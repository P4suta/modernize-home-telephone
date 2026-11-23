use crate::Limits;
use anyhow::{Context, anyhow, ensure};
use phone_core::directory::{
    CallerLabelText, DirectoryMatch, NationalNumber, caller_label, parse_directory_row,
};
use std::io::{BufRead, Read};

const FORMAT_LINE: &[u8] = b"public-phonebook-directory\t1";
const MAX_CONTACTS: u64 = 5_000_000;
const MAX_CATEGORIES: usize = 255;
const MAX_NAMES_PER_NUMBER: usize = 4096;

pub(crate) struct Labels {
    pub(crate) contacts: u64,
    pub(crate) numbers: u64,
    pub(crate) skipped: u64,
    pub(crate) entries: Vec<(NationalNumber, String)>,
}

struct Bounded<R> {
    inner: R,
    remaining: u64,
}

impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.remaining = self
            .remaining
            .checked_sub(read as u64)
            .ok_or_else(|| std::io::Error::other("decoded directory archive exceeds its limit"))?;
        Ok(read)
    }
}

fn next_line(reader: &mut impl BufRead, line: &mut Vec<u8>, limit: usize) -> anyhow::Result<bool> {
    line.clear();
    if reader.take(limit as u64 + 1).read_until(b'\n', line)? == 0 {
        return Ok(false);
    }
    ensure!(
        line.pop() == Some(b'\n'),
        "directory archive line exceeds its limit or lacks a line feed"
    );
    Ok(true)
}

pub(crate) fn read(compressed: &[u8], snapshot: &str, limits: Limits) -> anyhow::Result<Labels> {
    #[derive(serde::Deserialize)]
    struct Header {
        snapshot: String,
        contacts: u64,
        categories: Vec<Category>,
    }
    #[derive(serde::Deserialize)]
    struct Category {
        label: String,
    }
    let mut reader = std::io::BufReader::new(Bounded {
        inner: brotli_decompressor::Decompressor::new(compressed, 64 << 10),
        remaining: limits.decoded_bytes,
    });
    let mut line = Vec::new();
    ensure!(
        next_line(&mut reader, &mut line, FORMAT_LINE.len())? && line == FORMAT_LINE,
        "unsupported directory archive format"
    );
    ensure!(
        next_line(&mut reader, &mut line, limits.header_bytes)?,
        "directory archive header is missing"
    );
    let header: Header =
        serde_json::from_slice(&line).context("invalid directory archive header")?;
    ensure!(
        header.snapshot == snapshot,
        "directory archive snapshot differs from the listing"
    );
    ensure!(
        header.contacts <= MAX_CONTACTS,
        "directory archive contact count is outside its limit"
    );
    ensure!(
        !header.categories.is_empty()
            && header.categories.len() <= MAX_CATEGORIES
            && header
                .categories
                .iter()
                .all(|category| CallerLabelText::new(&category.label).is_ok()),
        "invalid directory archive categories"
    );
    let mut rows: Vec<(Box<str>, usize)> = Vec::new();
    let mut telephones: Vec<(NationalNumber, u32)> = Vec::new();
    while next_line(&mut reader, &mut line, limits.row_bytes)? {
        let position = rows.len() + 1;
        ensure!(
            (rows.len() as u64) < header.contacts,
            "directory archive row count differs from its header"
        );
        let text = std::str::from_utf8(&line)
            .with_context(|| format!("directory row {position} is not UTF-8"))?;
        let row = parse_directory_row(text)
            .map_err(|refusal| anyhow!("directory row {position} is invalid: {refusal:?}"))?;
        ensure!(
            row.category < header.categories.len(),
            "directory row {position} names an unknown category"
        );
        let index = u32::try_from(rows.len())?;
        for telephone in row.telephones() {
            let number = telephone
                .with_context(|| format!("directory row {position} has an incomplete telephone"))?;
            telephones.try_reserve(1)?;
            telephones.push((number, index));
        }
        rows.try_reserve(1)?;
        rows.push((row.name.into(), row.category));
    }
    ensure!(
        rows.len() as u64 == header.contacts,
        "directory archive row count differs from its header"
    );
    telephones.sort_unstable();
    telephones.dedup();
    let mut labels = Labels {
        contacts: header.contacts,
        numbers: 0,
        skipped: 0,
        entries: Vec::new(),
    };
    let mut matches = Vec::new();
    for group in telephones.chunk_by(|left, right| left.0 == right.0) {
        labels.numbers += 1;
        matches.clear();
        matches.extend(group.iter().map(|(_, index)| {
            let (name, category) = &rows[*index as usize];
            DirectoryMatch {
                name,
                category: &header.categories[*category].label,
            }
        }));
        let label = (matches.len() <= MAX_NAMES_PER_NUMBER)
            .then(|| caller_label(&matches))
            .flatten()
            .map(|label| label.to_string());
        match label {
            Some(label) if CallerLabelText::new(&label).is_ok() => {
                labels.entries.push((group[0].0, label));
            }
            _ => labels.skipped += 1,
        }
    }
    Ok(labels)
}
