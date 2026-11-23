use phone_core::{DialNumber, directory::NationalNumber};
use phone_plugin_phonebook::{INDEX_FILE, Limits, Report, Sync, lookup};
use sha2::Digest as _;
use std::sync::{Arc, Mutex};

const FIRST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const SECOND: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const CATEGORIES: &str = r#"[{"id":"hospital","label":"病院・診療所"},{"id":"care","label":"介護サービス"},{"id":"school","label":"学校"}]"#;

fn number(text: &str) -> NationalNumber {
    NationalNumber::from_dial(DialNumber::parse(text.as_bytes()).unwrap()).unwrap()
}

fn decoded(snapshot: &str, contacts: usize, rows: &[&str]) -> Vec<u8> {
    let mut text = format!(
        "public-phonebook-directory\t1\n{{\"snapshot\":\"{snapshot}\",\"generatedAt\":\"2026-11-04T14:45:49.809Z\",\"contacts\":{contacts},\"categories\":{CATEGORIES},\"prefectures\":[]}}\n"
    );
    for row in rows {
        text.push_str(row);
        text.push('\n');
    }
    text.into_bytes()
}

fn compressed(decoded: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut output, 4096, 9, 22);
        std::io::Write::write_all(&mut writer, decoded).unwrap();
    }
    output
}

const ROWS: [&str; 4] = [
    "0\t25208\tさくら病院\t滋賀県栗東市\t077-551-1234\u{1f}0775519999\u{1e}０７７５５１９９９９\t\t0\t0\t",
    "1\t25208\tさくら病院\t滋賀県栗東市\t077-551-1234\t\t0\t0\t",
    "2\t25208\t第一小学校\t滋賀県栗東市\t077-552-0001\t\t0\t0\t",
    "2\t25208\t第二小学校\t滋賀県栗東市\t077-552-0001\t\t0\t0\t",
];

#[derive(Clone)]
struct Served {
    listing: String,
    archive: Vec<u8>,
    downloads: usize,
}

struct Fixture {
    origin: String,
    served: Arc<Mutex<Served>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn listing(snapshot: &str, archive: &[u8], files: serde_json::Value) -> String {
    let digest = hex(&sha2::Sha256::digest(archive));
    let mut files = files;
    if files.is_null() {
        files = serde_json::json!([{
            "file": "directory.tsv.br",
            "format": "public-phonebook-directory/1",
            "sha256": digest,
            "bytes": archive.len(),
            "url": format!("/api/datasets/{snapshot}/{digest}/directory.tsv.br"),
        }]);
    }
    serde_json::json!({"snapshot": snapshot, "files": files}).to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn serve(snapshot: &str, decoded: &[u8]) -> Fixture {
    let archive = compressed(decoded);
    let served = Arc::new(Mutex::new(Served {
        listing: listing(snapshot, &archive, serde_json::Value::Null),
        archive,
        downloads: 0,
    }));
    let listed = served.clone();
    let downloaded = served.clone();
    let router = axum::Router::new()
        .route(
            "/api/datasets",
            axum::routing::get(move || {
                let body = listed.lock().unwrap().listing.clone();
                async move { ([("content-type", "application/json")], body) }
            }),
        )
        .route(
            "/api/datasets/{snapshot}/{digest}/directory.tsv.br",
            axum::routing::get(move || {
                let mut served = downloaded.lock().unwrap();
                served.downloads += 1;
                let body = served.archive.clone();
                async move { body }
            }),
        )
        .route(
            "/moved",
            axum::routing::get(|| async {
                axum::response::Redirect::temporary(&format!(
                    "/api/datasets/{FIRST}/{FIRST}/directory.tsv.br"
                ))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Fixture {
        origin: format!("http://{address}"),
        served,
        task,
    }
}

fn client(origin: &str) -> Sync {
    Sync::new(origin, true).unwrap()
}

#[tokio::test]
async fn a_sync_installs_directory_labels_and_skips_an_unchanged_snapshot() {
    let data = tempfile::tempdir().unwrap();
    let mut rows = ROWS.to_vec();
    rows.push("0\t25208\tCaf\u{1c}e\t滋賀県栗東市\t077-553-0001\t\t0\t0\t");
    let fixture = serve(FIRST, &decoded(FIRST, rows.len(), &rows)).await;
    assert_eq!(lookup(data.path(), number("0775511234")).unwrap(), None);
    let report = client(&fixture.origin).run(data.path()).await.unwrap();
    assert_eq!(
        report,
        Report::Installed {
            snapshot: FIRST.into(),
            contacts: 5,
            numbers: 4,
            labels: 3,
            skipped: 1,
        }
    );
    for (caller, label) in [
        (
            "0775511234",
            Some("さくら病院（介護サービス・病院・診療所）"),
        ),
        (
            "+81775511234",
            Some("さくら病院（介護サービス・病院・診療所）"),
        ),
        ("0775519999", Some("さくら病院（病院・診療所）")),
        ("0775520001", Some("公共施設（名前が2つあります）")),
        ("0775530001", None),
        ("0775530002", None),
    ] {
        assert_eq!(
            lookup(data.path(), number(caller)).unwrap().as_deref(),
            label,
            "{caller}"
        );
    }
    let installed = std::fs::read(data.path().join(INDEX_FILE)).unwrap();
    assert_eq!(
        client(&fixture.origin).run(data.path()).await.unwrap(),
        Report::Unchanged {
            snapshot: FIRST.into()
        }
    );
    assert_eq!(fixture.served.lock().unwrap().downloads, 1);
    assert_eq!(
        std::fs::read(data.path().join(INDEX_FILE)).unwrap(),
        installed
    );
    let mut names: Vec<_> = std::fs::read_dir(data.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [INDEX_FILE]);
}

enum Fault {
    Listing(serde_json::Value),
    Archive(Vec<u8>),
    Decoded(Vec<u8>),
    Limits(Limits),
}

#[tokio::test]
async fn a_failed_sync_keeps_the_previous_index_intact() {
    let digest = "a".repeat(64);
    let good = decoded(SECOND, ROWS.len(), &ROWS);
    let small = Limits {
        compressed_bytes: 64,
        ..Limits::default()
    };
    let cases: Vec<(&str, Fault)> = vec![
        (
            "missing format",
            Fault::Listing(
                serde_json::json!([{"file":"contacts.csv.gz","format":"public-phonebook-contacts-csv/1","sha256":digest,"bytes":1,"url":"/api/datasets/a/b/contacts.csv.gz"}]),
            ),
        ),
        (
            "foreign origin",
            Fault::Listing(
                serde_json::json!([{"file":"directory.tsv.br","format":"public-phonebook-directory/1","sha256":digest,"bytes":1,"url":"https://example.com/api/datasets/a/b/directory.tsv.br"}]),
            ),
        ),
        (
            "oversized listing",
            Fault::Listing(
                serde_json::json!([{"file":"directory.tsv.br","format":"public-phonebook-directory/1","sha256":digest,"bytes":65 * 1024 * 1024,"url":"/api/datasets/a/b/directory.tsv.br"}]),
            ),
        ),
        ("compressed limit", Fault::Limits(small)),
        (
            "decoded limit",
            Fault::Limits(Limits {
                decoded_bytes: 128,
                ..Limits::default()
            }),
        ),
        (
            "header limit",
            Fault::Limits(Limits {
                header_bytes: 64,
                ..Limits::default()
            }),
        ),
        (
            "row limit",
            Fault::Limits(Limits {
                row_bytes: 16,
                ..Limits::default()
            }),
        ),
        (
            "truncated archive",
            Fault::Archive(compressed(&good)[..20].to_vec()),
        ),
        (
            "changed archive",
            Fault::Archive({
                let mut bytes = compressed(&good);
                let last = bytes.len() - 1;
                bytes[last] ^= 1;
                bytes
            }),
        ),
        (
            "format line",
            Fault::Decoded(
                String::from_utf8(good.clone())
                    .unwrap()
                    .replacen("directory\t1", "directory\t2", 1)
                    .into_bytes(),
            ),
        ),
        (
            "snapshot",
            Fault::Decoded(decoded(FIRST, ROWS.len(), &ROWS)),
        ),
        ("row count", Fault::Decoded(decoded(SECOND, 3, &ROWS))),
        (
            "invalid row",
            Fault::Decoded(decoded(
                SECOND,
                1,
                &["0\t25208\t\t住所\t077-551-1234\t\t0\t0\t"],
            )),
        ),
        (
            "invalid telephone",
            Fault::Decoded(decoded(
                SECOND,
                1,
                &["0\t25208\t病院\t住所\t551-1234\t\t0\t0\t"],
            )),
        ),
        (
            "unknown category",
            Fault::Decoded(decoded(
                SECOND,
                1,
                &["3\t25208\t病院\t住所\t077-551-1234\t\t0\t0\t"],
            )),
        ),
        (
            "missing final line feed",
            Fault::Decoded(good[..good.len() - 1].to_vec()),
        ),
    ];
    for (name, fault) in cases {
        let data = tempfile::tempdir().unwrap();
        let fixture = serve(FIRST, &decoded(FIRST, 1, &ROWS[2..3])).await;
        client(&fixture.origin).run(data.path()).await.unwrap();
        let previous = std::fs::read(data.path().join(INDEX_FILE)).unwrap();
        let mut sync = client(&fixture.origin);
        {
            let mut served = fixture.served.lock().unwrap();
            served.archive = compressed(&good);
            served.listing = listing(SECOND, &served.archive, serde_json::Value::Null);
            match fault {
                Fault::Listing(files) => {
                    served.listing = listing(SECOND, &served.archive, files);
                }
                Fault::Archive(bytes) => served.archive = bytes,
                Fault::Decoded(bytes) => {
                    served.archive = compressed(&bytes);
                    served.listing = listing(SECOND, &served.archive, serde_json::Value::Null);
                }
                Fault::Limits(limits) => sync = sync.with_limits(limits),
            }
        }
        let error = sync.run(data.path()).await.unwrap_err();
        eprintln!("CASE {name}: {error:#}");
        assert_eq!(
            std::fs::read(data.path().join(INDEX_FILE)).unwrap(),
            previous,
            "{name}"
        );
        assert_eq!(
            lookup(data.path(), number("0775520001"))
                .unwrap()
                .as_deref(),
            Some("第一小学校（学校）"),
            "{name}"
        );
        assert_eq!(
            std::fs::read_dir(data.path()).unwrap().count(),
            1,
            "{name} left a partial index"
        );
    }
}

#[tokio::test]
async fn an_interrupted_partial_index_is_replaced_by_the_next_sync() {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join(format!("{INDEX_FILE}.partial")), b"torn").unwrap();
    let fixture = serve(FIRST, &decoded(FIRST, ROWS.len(), &ROWS)).await;
    client(&fixture.origin).run(data.path()).await.unwrap();
    assert!(!data.path().join(format!("{INDEX_FILE}.partial")).exists());
    assert!(lookup(data.path(), number("0775511234")).unwrap().is_some());
}

#[tokio::test]
async fn origins_must_be_https_or_loopback_fixtures_and_refuse_redirects() {
    assert!(Sync::new("https://www.dial.page", false).is_ok());
    for refused in [
        "http://www.dial.page",
        "http://127.0.0.1:8787",
        "https://www.dial.page/api",
        "https://user@www.dial.page",
        "https://www.dial.page/?a=1",
        "ftp://www.dial.page",
    ] {
        assert!(Sync::new(refused, false).is_err(), "{refused}");
    }
    assert!(Sync::new("http://www.dial.page", true).is_err());
    let data = tempfile::tempdir().unwrap();
    let fixture = serve(FIRST, &decoded(FIRST, ROWS.len(), &ROWS)).await;
    fixture.served.lock().unwrap().listing = String::new();
    assert!(client(&fixture.origin).run(data.path()).await.is_err());
    {
        let mut served = fixture.served.lock().unwrap();
        let files = serde_json::json!([{
            "file": "directory.tsv.br",
            "format": "public-phonebook-directory/1",
            "sha256": hex(&sha2::Sha256::digest(&served.archive)),
            "bytes": served.archive.len(),
            "url": "/moved",
        }]);
        served.listing = listing(FIRST, &served.archive, files);
    }
    assert!(client(&fixture.origin).run(data.path()).await.is_err());
    assert_eq!(fixture.served.lock().unwrap().downloads, 0);
    assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 0);
}

#[test]
fn lookups_distinguish_a_missing_index_from_a_damaged_one() {
    let data = tempfile::tempdir().unwrap();
    assert_eq!(lookup(data.path(), number("0775511234")).unwrap(), None);
    std::fs::write(data.path().join(INDEX_FILE), b"not a database").unwrap();
    assert!(lookup(data.path(), number("0775511234")).is_err());
}
