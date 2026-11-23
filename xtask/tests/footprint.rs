use std::collections::BTreeMap;
use xtask::footprint::{
    Budgets, Metric, Observation, Phase, Preparation, SampleError, ServiceBudget, Violation,
    evaluate, follows_evidence, observe, parse_sample, process_age_seconds,
};

const WINDOW_MS: u64 = 30_000;
const STAT: &str = "1 (phone server) S 0 1 1 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 7 0 50000 1 2 3\n";

#[test]
fn the_process_age_follows_uptime_and_the_start_tick() {
    assert_eq!(process_age_seconds("1000.50 3000.00\n", STAT), Ok(500));
}

#[test]
fn malformed_uptime_and_process_stat_are_refused() {
    for (uptime, stat, field) in [
        ("soon 3000.00\n", STAT, "uptime"),
        ("", STAT, "uptime"),
        ("1000.50 3000.00\n", "1 phone-server S 0", "starttime"),
        ("1000.50 3000.00\n", "1 (phone) S 0 1 1", "starttime"),
        ("100.00 3000.00\n", STAT, "starttime"),
    ] {
        assert_eq!(
            process_age_seconds(uptime, stat),
            Err(SampleError::InvalidValue(field)),
            "{uptime:?} {stat:?}"
        );
    }
}

#[test]
fn a_service_restarted_after_the_browser_calls_does_not_follow_them() {
    assert!(follows_evidence(600, 60));
    assert!(follows_evidence(60, 60));
    assert!(!follows_evidence(10, 60));
}

fn task(name: &str, id: u32, voluntary: u64, involuntary: u64) -> String {
    format!(
        "Name:\t{name}\nUmask:\t0022\nState:\tS (sleeping)\nTgid:\t1\nPid:\t{id}\nVmRSS:\t    9892 kB\nRssAnon:\t    4576 kB\nRssFile:\t    5316 kB\nThreads:\t3\nvoluntary_ctxt_switches:\t{voluntary}\nnonvoluntary_ctxt_switches:\t{involuntary}\n"
    )
}

fn budget() -> ServiceBudget {
    ServiceBudget {
        anonymous_kib: 5_000,
        threads: 4,
        switches_per_minute: 120,
    }
}

fn budgets() -> Budgets {
    let services: BTreeMap<_, _> = [
        ("app".to_owned(), budget()),
        ("gateway".to_owned(), budget()),
    ]
    .into();
    Budgets::parse(&format!(
        "settle_seconds = 15\nwindow_seconds = 30\n[images]\napp = 1000\ngateway = 2000\n{}{}",
        table("started", &services),
        table("settled", &services)
    ))
    .unwrap()
}

fn table(phase: &str, services: &BTreeMap<String, ServiceBudget>) -> String {
    services
        .iter()
        .map(|(name, budget)| {
            format!(
                "[{phase}.{name}]\nanonymous_kib = {}\nthreads = {}\nswitches_per_minute = {}\n",
                budget.anonymous_kib, budget.threads, budget.switches_per_minute
            )
        })
        .collect()
}

fn within() -> Observation {
    Observation {
        resident_kib: 10_000,
        anonymous_kib: 5_000,
        threads: 4,
        switches_per_minute: 120,
    }
}

fn services(app: Observation) -> BTreeMap<String, Observation> {
    [("app".to_owned(), app), ("gateway".to_owned(), within())].into()
}

fn images() -> BTreeMap<String, u64> {
    [("app".to_owned(), 1000), ("gateway".to_owned(), 2000)].into()
}

#[test]
fn the_started_phase_restarts_the_lab_and_the_settled_phase_follows_browser_calls() {
    assert_eq!(Phase::Started.preparation(), Preparation::Restart);
    assert_eq!(Phase::Settled.preparation(), Preparation::AfterBrowserCalls);
}

#[test]
fn a_status_sample_yields_process_memory_and_every_task() {
    let sample = parse_sample(&format!(
        "{}{}",
        task("phone-server", 1, 42, 14),
        task("tokio-rt-worker", 6, 100, 10)
    ))
    .unwrap();
    assert_eq!(sample.resident_kib, 9892);
    assert_eq!(sample.anonymous_kib, 4576);
    assert_eq!(
        sample
            .tasks
            .iter()
            .map(|task| (task.id, task.switches))
            .collect::<Vec<_>>(),
        [(1, 56), (6, 110)]
    );
}

#[test]
fn malformed_status_samples_are_refused_with_their_reason() {
    let main = task("phone-server", 1, 42, 14);
    let cases = [
        (String::new(), SampleError::Empty),
        (task("worker", 6, 1, 1), SampleError::MissingMainTask),
        (
            format!("Pid:\t1\n{main}"),
            SampleError::FieldOutsideTask("Pid".into()),
        ),
        (
            main.replace("nonvoluntary_ctxt_switches:\t14\n", ""),
            SampleError::MissingField("nonvoluntary_ctxt_switches"),
        ),
        (
            main.replace("RssAnon:\t    4576 kB\n", ""),
            SampleError::MissingField("RssAnon"),
        ),
        (
            main.replace("VmRSS:\t    9892 kB", "VmRSS:\t    9892 MB"),
            SampleError::InvalidValue("VmRSS"),
        ),
        (
            main.replace(
                "voluntary_ctxt_switches:\t42",
                "voluntary_ctxt_switches:\tmany",
            ),
            SampleError::InvalidValue("voluntary_ctxt_switches"),
        ),
        (format!("{main}{main}"), SampleError::DuplicateTask(1)),
    ];
    for (text, reason) in cases {
        assert_eq!(parse_sample(&text), Err(reason), "{text}");
    }
}

#[test]
fn the_switch_rate_counts_surviving_and_new_tasks_over_the_window() {
    let before = parse_sample(&format!(
        "{}{}{}",
        task("phone-server", 1, 40, 10),
        task("tokio-rt-worker", 6, 100, 0),
        task("exited", 9, 500, 0)
    ))
    .unwrap();
    let after = parse_sample(&format!(
        "{}{}{}",
        task("phone-server", 1, 41, 10),
        task("tokio-rt-worker", 6, 130, 0),
        task("spawned", 12, 4, 0)
    ))
    .unwrap();
    assert_eq!(
        observe(&before, &after, WINDOW_MS),
        Ok(Observation {
            resident_kib: 9892,
            anonymous_kib: 4576,
            threads: 3,
            switches_per_minute: 70,
        })
    );
}

#[test]
fn a_reused_task_identity_counts_from_zero_and_a_partial_minute_rounds_up() {
    let before = parse_sample(&task("phone-server", 1, 900, 0)).unwrap();
    let after = parse_sample(&task("phone-server", 1, 1, 0)).unwrap();
    assert_eq!(
        observe(&before, &after, 7_000).map(|observation| observation.switches_per_minute),
        Ok(9)
    );
}

#[test]
fn an_empty_window_is_refused() {
    let sample = parse_sample(&task("phone-server", 1, 1, 1)).unwrap();
    assert_eq!(observe(&sample, &sample, 0), Err(SampleError::EmptyWindow));
}

#[test]
fn observations_at_their_budgets_pass() {
    assert_eq!(
        evaluate(&budgets(), Phase::Started, &services(within()), &images()),
        Vec::<Violation>::new()
    );
}

#[test]
fn file_backed_resident_memory_is_reported_without_a_budget() {
    let observation = Observation {
        resident_kib: u64::MAX,
        ..within()
    };
    assert_eq!(
        evaluate(
            &budgets(),
            Phase::Started,
            &services(observation),
            &images()
        ),
        Vec::<Violation>::new()
    );
}

#[test]
fn each_budget_exceeded_by_one_unit_is_rejected() {
    let base = within();
    for (observation, metric, value, limit) in [
        (
            Observation {
                anonymous_kib: base.anonymous_kib + 1,
                ..base
            },
            Metric::AnonymousKib,
            5_001,
            5_000,
        ),
        (
            Observation {
                threads: base.threads + 1,
                ..base
            },
            Metric::Threads,
            5,
            4,
        ),
        (
            Observation {
                switches_per_minute: base.switches_per_minute + 1,
                ..base
            },
            Metric::SwitchesPerMinute,
            121,
            120,
        ),
    ] {
        assert_eq!(
            evaluate(
                &budgets(),
                Phase::Settled,
                &services(observation),
                &images()
            ),
            [Violation::Exceeded {
                subject: "app".into(),
                metric,
                value,
                limit
            }]
        );
    }
    let mut large = images();
    large.insert("gateway".into(), 2001);
    assert_eq!(
        evaluate(&budgets(), Phase::Started, &services(within()), &large),
        [Violation::Exceeded {
            subject: "gateway".into(),
            metric: Metric::ImageBytes,
            value: 2001,
            limit: 2000
        }]
    );
}

#[test]
fn missing_and_unbudgeted_services_and_images_are_rejected() {
    let mut observed = services(within());
    observed.remove("gateway");
    observed.insert("console".into(), within());
    let mut sizes = images();
    sizes.remove("app");
    sizes.insert("browser".into(), 1);
    assert_eq!(
        evaluate(&budgets(), Phase::Started, &observed, &sizes),
        [
            Violation::MissingService("gateway".into()),
            Violation::UnbudgetedService("console".into()),
            Violation::MissingImage("app".into()),
            Violation::UnbudgetedImage("browser".into()),
        ]
    );
}

#[test]
fn budget_files_without_a_window_services_or_images_are_refused() {
    let complete = "settle_seconds = 15\nwindow_seconds = 30\n[images]\napp = 1\n[started.app]\nanonymous_kib = 1\nthreads = 1\nswitches_per_minute = 1\n[settled.app]\nanonymous_kib = 1\nthreads = 1\nswitches_per_minute = 1\n";
    Budgets::parse(complete).unwrap();
    for incomplete in [
        complete.replace("window_seconds = 30", "window_seconds = 0"),
        complete.replace("[images]\napp = 1\n", "[images]\n"),
        complete.replace(
            "[started.app]\nanonymous_kib = 1\nthreads = 1\nswitches_per_minute = 1\n",
            "[started]\n",
        ),
        complete.replacen("threads = 1\n", "threads = 1\nwakeups = 1\n", 1),
    ] {
        assert!(Budgets::parse(&incomplete).is_err(), "{incomplete}");
    }
}

#[test]
fn the_maintained_budgets_cover_the_lab_services_in_both_phases() {
    let budgets =
        Budgets::parse(&std::fs::read_to_string("../verification/footprint.toml").unwrap())
            .unwrap();
    let expected = ["app", "gateway"];
    assert!(budgets.images.keys().eq(expected));
    for phase in [Phase::Started, Phase::Settled] {
        assert!(budgets.services(phase).keys().eq(expected), "{phase:?}");
    }
}
