#![cfg(feature = "upstream-archives")]

mod support;

use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

fn dockerfile() -> String {
    std::fs::read_to_string(root().join("deploy/asterisk/Dockerfile")).unwrap()
}

fn build(source: &str, output: &Path) -> Output {
    let mut child = Command::new("docker")
        .args([
            "build",
            "--no-cache",
            "--progress=plain",
            "--platform=linux/arm64",
            "--target=source-inputs",
            "--output",
        ])
        .arg(format!("type=local,dest={}", output.display()))
        .args(["-f", "-", "."])
        .current_dir(root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn diagnostics(result: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    )
}

struct ArchiveServer {
    address: std::net::SocketAddr,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ArchiveServer {
    fn start() -> Self {
        let listener = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let stopping = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stopping);
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                let mut stream = stream.unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let request = support::read_request(&mut stream);
                let request = std::str::from_utf8(&request).unwrap();
                let mut words = request.split_whitespace();
                let method = words.next().unwrap();
                let path = words.next().unwrap();
                assert!(["HEAD", "GET"].contains(&method), "{request}");
                let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
                assert_eq!(parts.len(), 3, "{path}");
                let mut bytes = fixture_bytes(parts[1], parts[2]);
                if parts[0] == "corrupt" {
                    bytes.extend_from_slice(b"corrupted");
                } else {
                    assert_eq!(parts[0], "valid", "{path}");
                }
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                if method == "GET" {
                    stream.write_all(&bytes).unwrap();
                }
            }
        });
        Self {
            address,
            stopping,
            thread: Some(thread),
        }
    }

    fn url(&self, corrupted: bool, id: &str, name: &str) -> String {
        let host = if cfg!(target_os = "macos") {
            "host.docker.internal"
        } else {
            "127.0.0.1"
        };
        let variant = if corrupted { "corrupt" } else { "valid" };
        format!(
            "http://{host}:{}/{variant}/{id}/{name}",
            self.address.port()
        )
    }
}

impl Drop for ArchiveServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.address.port()));
        if !std::thread::panicking() {
            self.thread.take().unwrap().join().unwrap();
        }
    }
}

fn fixture_bytes(id: &str, name: &str) -> Vec<u8> {
    format!("source archive positive control {id}/{name}\n").into_bytes()
}

struct Inputs {
    source: String,
    exported: BTreeMap<String, Vec<u8>>,
}

fn controlled_inputs(server: &ArchiveServer, corrupted: Option<&str>, unchecked: bool) -> Inputs {
    let id = uuid::Uuid::new_v4().to_string();
    let mut exported = BTreeMap::new();
    let source = dockerfile()
        .lines()
        .map(|line| {
            if !line.starts_with("ADD ") {
                return line.to_owned();
            }
            let words: Vec<_> = line.split_whitespace().collect();
            assert_eq!(words.len(), 4, "unexpected source instruction: {line}");
            let checksum = words[1];
            assert!(checksum.starts_with("--checksum=sha256:"), "{line}");
            let name = Path::new(words[3]).file_name().unwrap().to_str().unwrap();
            let selected = corrupted == Some(name);
            let mut bytes = fixture_bytes(&id, name);
            let digest = format!("{:x}", Sha256::digest(&bytes));
            if selected {
                bytes.extend_from_slice(b"corrupted");
            }
            exported.insert(name.to_owned(), bytes);
            let replacement = if unchecked && selected {
                String::new()
            } else {
                format!("--checksum=sha256:{digest} ")
            };
            line.replace(&format!("{checksum} "), &replacement)
                .replace(words[2], &server.url(selected, &id, name))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(exported.len(), 2);
    Inputs { source, exported }
}

fn corruption_controls(name: &str) {
    let server = ArchiveServer::start();
    let directory = tempfile::tempdir().unwrap();
    let inputs = controlled_inputs(&server, None, false);
    let valid = build(&inputs.source, &directory.path().join("valid"));
    assert!(valid.status.success(), "{}", diagnostics(&valid));
    assert_eq!(
        std::fs::read(directory.path().join("valid").join(name)).unwrap(),
        inputs.exported[name]
    );
    let inputs = controlled_inputs(&server, Some(name), false);
    let corrupt = build(&inputs.source, &directory.path().join("corrupt"));
    let diagnostic = diagnostics(&corrupt);
    assert!(!corrupt.status.success(), "{diagnostic}");
    assert!(
        diagnostic.contains("digest mismatch") || diagnostic.contains("checksum mismatch"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains(&format!("{:x}", Sha256::digest(&inputs.exported[name]))),
        "the rejected bytes must identify the injected corruption: {diagnostic}"
    );
    let inputs = controlled_inputs(&server, Some(name), true);
    let unchecked = build(&inputs.source, &directory.path().join("unchecked"));
    assert!(unchecked.status.success(), "{}", diagnostics(&unchecked));
    assert_eq!(
        std::fs::read(directory.path().join("unchecked").join(name)).unwrap(),
        inputs.exported[name]
    );
}

#[test]
fn upstream_archives_have_a_buildable_source_only_target() {
    let directory = tempfile::tempdir().unwrap();
    let source = dockerfile();
    let result = build(&source, directory.path());
    assert!(result.status.success(), "{}", diagnostics(&result));
    let instructions: Vec<_> = source
        .lines()
        .filter(|line| line.starts_with("ADD "))
        .collect();
    assert_eq!(instructions.len(), 2);
    for line in instructions {
        let words: Vec<_> = line.split_whitespace().collect();
        let digest = words[1].strip_prefix("--checksum=sha256:").unwrap();
        let file = directory
            .path()
            .join(Path::new(words[3]).file_name().unwrap());
        assert_eq!(xtask::bundle::digest(&file).unwrap(), digest);
    }
}

#[test]
fn asterisk_archive_rejects_corruption_and_requires_its_checksum() {
    corruption_controls("asterisk.tar.gz");
}

#[test]
fn japanese_archive_rejects_corruption_and_requires_its_checksum() {
    corruption_controls("japanese.tar.gz");
}
