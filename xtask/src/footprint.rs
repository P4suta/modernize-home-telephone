const MAIN_TASK: u32 = 1;
const MILLISECONDS_PER_MINUTE: u64 = 60_000;
const USER_HZ: u64 = 100;
const START_TIME_AFTER_COMMAND: usize = 19;

use anyhow::ensure;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum Phase {
    Started,
    Settled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preparation {
    Restart,
    AfterBrowserCalls,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Settled => "settled",
        }
    }
    pub fn preparation(self) -> Preparation {
        match self {
            Self::Started => Preparation::Restart,
            Self::Settled => Preparation::AfterBrowserCalls,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceBudget {
    pub anonymous_kib: u64,
    pub threads: u64,
    pub switches_per_minute: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    pub settle_seconds: u64,
    pub window_seconds: u64,
    pub images: BTreeMap<String, u64>,
    started: BTreeMap<String, ServiceBudget>,
    settled: BTreeMap<String, ServiceBudget>,
}

impl Budgets {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let budgets: Self = toml::from_str(text)?;
        ensure!(
            budgets.window_seconds > 0,
            "the measurement window is empty"
        );
        ensure!(!budgets.images.is_empty(), "no image has a budget");
        ensure!(
            !budgets.started.is_empty() && !budgets.settled.is_empty(),
            "a phase has no service budget"
        );
        Ok(budgets)
    }
    pub fn services(&self, phase: Phase) -> &BTreeMap<String, ServiceBudget> {
        match phase {
            Phase::Started => &self.started,
            Phase::Settled => &self.settled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: u32,
    pub switches: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub resident_kib: u64,
    pub anonymous_kib: u64,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SampleError {
    Empty,
    MissingMainTask,
    FieldOutsideTask(String),
    MissingField(&'static str),
    InvalidValue(&'static str),
    DuplicateTask(u32),
    EmptyWindow,
}

impl std::fmt::Display for SampleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(formatter, "the process status sample is empty"),
            Self::MissingMainTask => write!(formatter, "the sample has no main task"),
            Self::FieldOutsideTask(field) => write!(formatter, "{field} precedes every task"),
            Self::MissingField(field) => write!(formatter, "a task has no {field}"),
            Self::InvalidValue(field) => write!(formatter, "{field} has an invalid value"),
            Self::DuplicateTask(id) => write!(formatter, "task {id} appears twice"),
            Self::EmptyWindow => write!(formatter, "the measurement window is empty"),
        }
    }
}

impl std::error::Error for SampleError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Observation {
    pub resident_kib: u64,
    pub anonymous_kib: u64,
    pub threads: u64,
    pub switches_per_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    AnonymousKib,
    Threads,
    SwitchesPerMinute,
    ImageBytes,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Violation {
    Exceeded {
        subject: String,
        metric: Metric,
        value: u64,
        limit: u64,
    },
    MissingService(String),
    UnbudgetedService(String),
    MissingImage(String),
    UnbudgetedImage(String),
}

impl std::fmt::Display for Violation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exceeded {
                subject,
                metric,
                value,
                limit,
            } => write!(formatter, "{subject} {metric:?} is {value}, above {limit}"),
            Self::MissingService(name) => {
                write!(formatter, "budgeted service {name} is not running")
            }
            Self::UnbudgetedService(name) => write!(formatter, "service {name} has no budget"),
            Self::MissingImage(name) => write!(formatter, "budgeted image {name} is missing"),
            Self::UnbudgetedImage(name) => write!(formatter, "image {name} has no budget"),
        }
    }
}

#[derive(Default)]
struct Record {
    id: Option<u32>,
    resident_kib: Option<u64>,
    anonymous_kib: Option<u64>,
    voluntary: Option<u64>,
    involuntary: Option<u64>,
}

fn count(value: &str, field: &'static str) -> Result<u64, SampleError> {
    value
        .trim()
        .parse()
        .map_err(|_| SampleError::InvalidValue(field))
}

fn kibibytes(value: &str, field: &'static str) -> Result<u64, SampleError> {
    value
        .trim()
        .strip_suffix(" kB")
        .ok_or(SampleError::InvalidValue(field))
        .and_then(|number| count(number, field))
}

pub fn parse_sample(text: &str) -> Result<Sample, SampleError> {
    let mut records: Vec<Record> = vec![];
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key == "Name" {
            records.push(Record::default());
            continue;
        }
        let Some(record) = records.last_mut() else {
            return Err(SampleError::FieldOutsideTask(key.into()));
        };
        match key {
            "Pid" => {
                record.id = Some(
                    value
                        .trim()
                        .parse()
                        .map_err(|_| SampleError::InvalidValue("Pid"))?,
                );
            }
            "VmRSS" => record.resident_kib = Some(kibibytes(value, "VmRSS")?),
            "RssAnon" => record.anonymous_kib = Some(kibibytes(value, "RssAnon")?),
            "voluntary_ctxt_switches" => {
                record.voluntary = Some(count(value, "voluntary_ctxt_switches")?);
            }
            "nonvoluntary_ctxt_switches" => {
                record.involuntary = Some(count(value, "nonvoluntary_ctxt_switches")?);
            }
            _ => {}
        }
    }
    if records.is_empty() {
        return Err(SampleError::Empty);
    }
    let mut tasks = Vec::with_capacity(records.len());
    let mut memory = None;
    for record in records {
        let id = record.id.ok_or(SampleError::MissingField("Pid"))?;
        let voluntary = record
            .voluntary
            .ok_or(SampleError::MissingField("voluntary_ctxt_switches"))?;
        let involuntary = record
            .involuntary
            .ok_or(SampleError::MissingField("nonvoluntary_ctxt_switches"))?;
        if tasks.iter().any(|task: &Task| task.id == id) {
            return Err(SampleError::DuplicateTask(id));
        }
        if id == MAIN_TASK {
            memory = Some((
                record
                    .resident_kib
                    .ok_or(SampleError::MissingField("VmRSS"))?,
                record
                    .anonymous_kib
                    .ok_or(SampleError::MissingField("RssAnon"))?,
            ));
        }
        tasks.push(Task {
            id,
            switches: voluntary.saturating_add(involuntary),
        });
    }
    let (resident_kib, anonymous_kib) = memory.ok_or(SampleError::MissingMainTask)?;
    Ok(Sample {
        resident_kib,
        anonymous_kib,
        tasks,
    })
}

pub fn process_age_seconds(uptime: &str, stat: &str) -> Result<u64, SampleError> {
    let uptime = uptime
        .split_whitespace()
        .next()
        .and_then(|seconds| seconds.split_once('.'))
        .and_then(|(seconds, _)| seconds.parse::<u64>().ok())
        .ok_or(SampleError::InvalidValue("uptime"))?;
    let started = stat
        .rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(START_TIME_AFTER_COMMAND))
        .and_then(|ticks| ticks.parse::<u64>().ok())
        .map(|ticks| ticks / USER_HZ)
        .ok_or(SampleError::InvalidValue("starttime"))?;
    uptime
        .checked_sub(started)
        .ok_or(SampleError::InvalidValue("starttime"))
}

pub fn follows_evidence(process_age_seconds: u64, evidence_age_seconds: u64) -> bool {
    process_age_seconds >= evidence_age_seconds
}

pub fn observe(
    before: &Sample,
    after: &Sample,
    window_ms: u64,
) -> Result<Observation, SampleError> {
    if window_ms == 0 {
        return Err(SampleError::EmptyWindow);
    }
    let switches = after
        .tasks
        .iter()
        .map(|task| {
            before
                .tasks
                .iter()
                .find(|earlier| earlier.id == task.id)
                .and_then(|earlier| task.switches.checked_sub(earlier.switches))
                .unwrap_or(task.switches)
        })
        .fold(0_u64, u64::saturating_add);
    Ok(Observation {
        resident_kib: after.resident_kib,
        anonymous_kib: after.anonymous_kib,
        threads: after.tasks.len() as u64,
        switches_per_minute: switches
            .saturating_mul(MILLISECONDS_PER_MINUTE)
            .div_ceil(window_ms),
    })
}

fn exceeded(subject: &str, metric: Metric, value: u64, limit: u64) -> Option<Violation> {
    (value > limit).then(|| Violation::Exceeded {
        subject: subject.into(),
        metric,
        value,
        limit,
    })
}

pub fn evaluate(
    budgets: &Budgets,
    phase: Phase,
    services: &BTreeMap<String, Observation>,
    images: &BTreeMap<String, u64>,
) -> Vec<Violation> {
    let mut violations = vec![];
    let budgeted = budgets.services(phase);
    for (name, budget) in budgeted {
        let Some(observed) = services.get(name) else {
            violations.push(Violation::MissingService(name.clone()));
            continue;
        };
        violations.extend(
            [
                exceeded(
                    name,
                    Metric::AnonymousKib,
                    observed.anonymous_kib,
                    budget.anonymous_kib,
                ),
                exceeded(name, Metric::Threads, observed.threads, budget.threads),
                exceeded(
                    name,
                    Metric::SwitchesPerMinute,
                    observed.switches_per_minute,
                    budget.switches_per_minute,
                ),
            ]
            .into_iter()
            .flatten(),
        );
    }
    violations.extend(
        services
            .keys()
            .filter(|name| !budgeted.contains_key(*name))
            .map(|name| Violation::UnbudgetedService(name.clone())),
    );
    for (name, limit) in &budgets.images {
        match images.get(name) {
            Some(size) => violations.extend(exceeded(name, Metric::ImageBytes, *size, *limit)),
            None => violations.push(Violation::MissingImage(name.clone())),
        }
    }
    violations.extend(
        images
            .keys()
            .filter(|name| !budgets.images.contains_key(*name))
            .map(|name| Violation::UnbudgetedImage(name.clone())),
    );
    violations
}
