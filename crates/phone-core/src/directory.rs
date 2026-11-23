use crate::DialNumber;
use core::{fmt, num::NonZeroUsize};

pub const MAX_LABEL_BYTES: usize = 512;
const MIN_KEY: u64 = 100_000_000;
const MAX_KEY: u64 = 9_999_999_999;
const MAX_SUBSCRIBER_DIGITS: usize = 10;
const NATIONAL_SEPARATOR: char = '\u{1e}';
const TELEPHONE_SEPARATOR: char = '\u{1f}';
const ROW_FIELDS: usize = 9;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NationalNumber(u64);

impl NationalNumber {
    pub fn from_dial(number: DialNumber<'_>) -> Option<Self> {
        let subscriber = match number.bytes() {
            [b'+', b'8', b'1', rest @ ..] => rest,
            [b'0', rest @ ..] => rest,
            _ => return None,
        };
        Self::from_subscriber_digits(subscriber.iter().copied())
    }

    pub const fn from_key(key: u64) -> Option<Self> {
        if key >= MIN_KEY && key <= MAX_KEY {
            Some(Self(key))
        } else {
            None
        }
    }

    pub const fn key(self) -> u64 {
        self.0
    }

    fn from_subscriber_digits(digits: impl Iterator<Item = u8>) -> Option<Self> {
        let mut key: u64 = 0;
        for (count, digit) in digits.enumerate() {
            if !digit.is_ascii_digit()
                || count == MAX_SUBSCRIBER_DIGITS
                || (count == 0 && digit == b'0')
            {
                return None;
            }
            key = key * 10 + u64::from(digit - b'0');
        }
        Self::from_key(key)
    }
}

impl fmt::Display for NationalNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "0{}", self.0)
    }
}

pub fn directory_phone_key(telephone: &str) -> Option<NationalNumber> {
    let mut parts = telephone.split(NATIONAL_SEPARATOR);
    let first = parts.next()?;
    match (parts.next(), parts.next()) {
        (None, _) => national_digits(first.bytes().filter(u8::is_ascii_digit)),
        (Some(spelling), None) if !spelling.is_empty() => national_digits(first.bytes()),
        _ => None,
    }
}

fn national_digits(mut digits: impl Iterator<Item = u8>) -> Option<NationalNumber> {
    if digits.next()? != b'0' {
        return None;
    }
    NationalNumber::from_subscriber_digits(digits)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowRefusal {
    FieldCount,
    Category,
    Name,
    Telephones,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryRow<'a> {
    pub category: usize,
    pub name: &'a str,
    telephones: &'a str,
}

impl<'a> DirectoryRow<'a> {
    pub fn telephones(&self) -> impl Iterator<Item = Option<NationalNumber>> + 'a {
        self.telephones
            .split(TELEPHONE_SEPARATOR)
            .map(directory_phone_key)
    }
}

pub fn parse_directory_row(line: &str) -> Result<DirectoryRow<'_>, RowRefusal> {
    let mut fields: [&str; ROW_FIELDS] = [""; ROW_FIELDS];
    let mut count = 0;
    for field in line.split('\t') {
        if count == ROW_FIELDS {
            return Err(RowRefusal::FieldCount);
        }
        fields[count] = field;
        count += 1;
    }
    if count != ROW_FIELDS {
        return Err(RowRefusal::FieldCount);
    }
    let category = decimal(fields[0]).ok_or(RowRefusal::Category)?;
    if fields[2].is_empty() {
        return Err(RowRefusal::Name);
    }
    if fields[4].is_empty() {
        return Err(RowRefusal::Telephones);
    }
    Ok(DirectoryRow {
        category,
        name: fields[2],
        telephones: fields[4],
    })
}

fn decimal(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || (bytes.len() > 1 && bytes[0] == b'0') {
        return None;
    }
    let mut value: usize = 0;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add(usize::from(byte - b'0'))?;
    }
    Some(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryMatch<'a> {
    pub name: &'a str,
    pub category: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallerLabel<'a> {
    Named {
        name: &'a str,
        matches: &'a [DirectoryMatch<'a>],
    },
    Shared {
        names: NonZeroUsize,
    },
}

pub fn caller_label<'a>(matches: &'a [DirectoryMatch<'a>]) -> Option<CallerLabel<'a>> {
    let first = matches.first()?;
    let names = matches
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            matches[..*index]
                .iter()
                .all(|earlier| earlier.name != candidate.name)
        })
        .count();
    match NonZeroUsize::new(names) {
        Some(names) if names.get() > 1 => Some(CallerLabel::Shared { names }),
        _ => Some(CallerLabel::Named {
            name: first.name,
            matches,
        }),
    }
}

impl fmt::Display for CallerLabel<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shared { names } => write!(formatter, "公共施設（名前が{names}つあります）"),
            Self::Named { name, matches } => {
                write!(formatter, "{name}（")?;
                let mut previous: Option<&str> = None;
                while let Some(category) = matches
                    .iter()
                    .map(|candidate| candidate.category)
                    .filter(|category| previous.is_none_or(|previous| *category > previous))
                    .min()
                {
                    if previous.is_some() {
                        formatter.write_str("・")?;
                    }
                    formatter.write_str(category)?;
                    previous = Some(category);
                }
                formatter.write_str("）")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelRefusal {
    Length,
    Control,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallerLabelText<'a>(&'a str);

impl<'a> CallerLabelText<'a> {
    pub fn new(text: &'a str) -> Result<Self, LabelRefusal> {
        if text.is_empty() || text.len() > MAX_LABEL_BYTES {
            return Err(LabelRefusal::Length);
        }
        if text.chars().any(char::is_control) {
            return Err(LabelRefusal::Control);
        }
        Ok(Self(text))
    }

    pub const fn as_str(self) -> &'a str {
        self.0
    }
}
