use anyhow::{Context, ensure};
use futures_util::StreamExt;
use phone_core::directory::NationalNumber;
use sha2::Digest as _;
use std::{path::Path, time::Duration};

mod archive;
mod index;

pub use index::{install, lookup};

pub const INDEX_FILE: &str = "phonebook.sqlite";
pub const DEFAULT_ORIGIN: &str = "https://www.dial.page";
const ARCHIVE_FORMAT: &str = "public-phonebook-directory/1";
const MAX_LISTING_BYTES: u64 = 1 << 20;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub compressed_bytes: u64,
    pub decoded_bytes: u64,
    pub header_bytes: usize,
    pub row_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            compressed_bytes: 64 << 20,
            decoded_bytes: 256 << 20,
            header_bytes: 4 << 20,
            row_bytes: 64 << 10,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Report {
    Unchanged {
        snapshot: String,
    },
    Installed {
        snapshot: String,
        contacts: u64,
        numbers: u64,
        labels: u64,
        skipped: u64,
    },
}

pub struct Sync {
    client: reqwest::Client,
    origin: url::Url,
    limits: Limits,
}

struct Selected {
    snapshot: String,
    sha256: String,
    bytes: u64,
    url: url::Url,
}

impl Sync {
    pub fn new(origin: &str, allow_loopback: bool) -> anyhow::Result<Self> {
        let origin = url::Url::parse(origin)?;
        let local = match origin.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        ensure!(
            (origin.scheme() == "https" || (allow_loopback && origin.scheme() == "http" && local))
                && origin.host_str().is_some()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "invalid Public Phonebook origin"
        );
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            origin,
            limits: Limits::default(),
        })
    }

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub async fn run(&self, data_dir: &Path) -> anyhow::Result<Report> {
        let selected = self.select().await?;
        if index::installed(data_dir).is_some_and(|(snapshot, sha256)| {
            snapshot == selected.snapshot && sha256 == selected.sha256
        }) {
            return Ok(Report::Unchanged {
                snapshot: selected.snapshot,
            });
        }
        let response = self.client.get(selected.url.clone()).send().await?;
        ensure!(
            response.status() == reqwest::StatusCode::OK,
            "directory archive download failed"
        );
        let compressed = bounded(response, selected.bytes).await?;
        ensure!(
            u64::try_from(compressed.len())? == selected.bytes,
            "directory archive length differs from the listing"
        );
        ensure!(
            hex(&sha2::Sha256::digest(&compressed)) == selected.sha256,
            "directory archive digest differs from the listing"
        );
        let data_dir = data_dir.to_owned();
        let limits = self.limits;
        tokio::task::spawn_blocking(move || {
            let labels = archive::read(&compressed, &selected.snapshot, limits)?;
            index::install(
                &data_dir,
                &selected.snapshot,
                &selected.sha256,
                labels.contacts,
                &labels.entries,
            )?;
            Ok(Report::Installed {
                snapshot: selected.snapshot,
                contacts: labels.contacts,
                numbers: labels.numbers,
                labels: u64::try_from(labels.entries.len())?,
                skipped: labels.skipped,
            })
        })
        .await?
    }

    async fn select(&self) -> anyhow::Result<Selected> {
        #[derive(serde::Deserialize)]
        struct Listing {
            snapshot: String,
            files: Vec<File>,
        }
        #[derive(serde::Deserialize)]
        struct File {
            format: String,
            sha256: String,
            bytes: u64,
            url: String,
        }
        let response = self
            .client
            .get(self.origin.join("api/datasets")?)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        ensure!(
            response.status() == reqwest::StatusCode::OK,
            "dataset listing failed"
        );
        let listing: Listing = serde_json::from_slice(&bounded(response, MAX_LISTING_BYTES).await?)
            .context("invalid dataset listing")?;
        let mut archives = listing
            .files
            .into_iter()
            .filter(|file| file.format == ARCHIVE_FORMAT);
        let file = archives
            .next()
            .context("the dataset listing has no directory archive")?;
        ensure!(
            archives.next().is_none(),
            "the dataset listing repeats the directory archive"
        );
        ensure!(
            digest(&listing.snapshot) && digest(&file.sha256),
            "invalid dataset listing identity"
        );
        ensure!(
            file.bytes > 0 && file.bytes <= self.limits.compressed_bytes,
            "directory archive size is outside its limit"
        );
        let url = self.origin.join(&file.url)?;
        ensure!(
            url.origin() == self.origin.origin()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "directory archive is outside the dataset origin"
        );
        Ok(Selected {
            snapshot: listing.snapshot,
            sha256: file.sha256,
            bytes: file.bytes,
            url,
        })
    }
}

pub fn label(data_dir: &Path, caller: &str) -> anyhow::Result<Option<String>> {
    let Ok(number) = phone_core::DialNumber::parse(caller.as_bytes()) else {
        return Ok(None);
    };
    match NationalNumber::from_dial(number) {
        Some(number) => lookup(data_dir, number),
        None => Ok(None),
    }
}

async fn bounded(response: reqwest::Response, limit: u64) -> anyhow::Result<Vec<u8>> {
    let limit = usize::try_from(limit)?;
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= limit as u64),
        "HTTP response exceeds its byte limit"
    );
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            phone_core::bytes::bounded_add(bytes.len(), chunk.len(), limit).is_some(),
            "HTTP response exceeds its byte limit"
        );
        bytes.try_reserve(chunk.len())?;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
