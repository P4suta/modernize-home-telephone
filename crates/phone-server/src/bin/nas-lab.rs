use anyhow::{Context, ensure};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{any, get},
};
use phone_server::{
    app::router,
    config::{CallRecording, Config, Nas},
    controller::{Control, now},
    gateway::Ari,
    model::{Call, Phase, Recording},
    postcall::Processor,
    recordings::TemporaryAudio,
    secrets::Secrets,
    store::{ArchiveTask, ArchiveWork, Store},
};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};
use tower::ServiceExt;
use uuid::Uuid;

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("NAS lab cleanup failed: {}", error.kind());
        }
    }
}

struct Server {
    config: PathBuf,
    port: u16,
    child: Option<Child>,
}
impl Server {
    async fn start(&mut self) -> anyhow::Result<()> {
        ensure!(self.child.is_none(), "SMB fixture already runs");
        let child = Command::new("/usr/sbin/smbd")
            .args([
                "--foreground",
                "--no-process-group",
                "--debug-stdout",
                "--configfile",
            ])
            .arg(&self.config)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        self.child = Some(child);
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, self.port))
                    .await
                    .is_ok()
                {
                    return Ok(());
                }
                ensure!(
                    self.child
                        .as_mut()
                        .context("SMB fixture stopped")?
                        .try_wait()?
                        .is_none(),
                    "SMB fixture exited before readiness"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;
        Ok(())
    }
    async fn stop(&mut self) -> anyhow::Result<()> {
        self.child
            .take()
            .context("SMB fixture is absent")?
            .kill()
            .await?;
        Ok(())
    }
}

struct System {
    store: Store,
    processor: Processor,
    control: Control,
    app: Router,
}
impl System {
    async fn open(config: &Config, secrets: &Secrets) -> anyhow::Result<Self> {
        let store = Store::open(&config.data_dir.join("phone.sqlite")).await?;
        let ari = Ari::new(config.ari.clone(), secrets.ari_password.clone())?;
        let control = Control::start(config.clone(), ari.clone(), store.clone());
        tokio::time::timeout(Duration::from_secs(10), control.wait_ready()).await??;
        let processor = Processor::new(config.clone(), ari.clone(), store.clone(), None)?
            .with_archive_credentials(secrets.nas_credentials.clone());
        let app = router(config.clone(), control.clone(), ari, secrets.clone());
        Ok(Self {
            store,
            processor,
            control,
            app,
        })
    }
    async fn close(self) {
        let Self {
            store,
            processor,
            control,
            app,
        } = self;
        drop(app);
        control.shutdown().await;
        drop(control);
        drop(processor);
        drop(store);
    }
    async fn request(
        &self,
        id: Uuid,
        method: &str,
        range: Option<&str>,
    ) -> anyhow::Result<(StatusCode, Vec<u8>)> {
        let path = if method == "DELETE" {
            format!("/api/calls/{id}")
        } else {
            format!("/api/calls/{id}/recording")
        };
        let mut request = Request::builder()
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                12345,
            ))))
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", "a".repeat(32)));
        if let Some(range) = range {
            request = request.header("range", range);
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::empty())?)
            .await?;
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1048576).await?;
        Ok((status, body.to_vec()))
    }
    async fn capture(&self) -> anyhow::Result<Call> {
        let mut call = Call::offered("123".into(), "inbound".into(), 10);
        call.phase = Phase::Ended;
        call.ended_at = Some(20);
        call.recording = Recording::Pending;
        self.store.save(&call, "ended", 20).await?;
        self.processor.process(call.id).await?;
        let saved = self
            .store
            .get(call.id)
            .await?
            .context("captured call is missing")?;
        ensure!(
            matches!(saved.recording, Recording::Available { .. })
                && saved.processing == "complete"
                && saved.transcript.is_empty(),
            "capture requires no AI inference"
        );
        Ok(saved)
    }
    async fn task(&self, id: Uuid) -> anyhow::Result<ArchiveTask> {
        self.store
            .archive_tasks(now() + 3600)
            .await?
            .into_iter()
            .find(|task| task.id == id)
            .context("archive intent is missing")
    }
}

fn wav() -> anyhow::Result<Vec<u8>> {
    let mut data = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut data,
        hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for index in 0..8000_i32 {
        writer.write_sample(((index % 80) * 100 - 4000) as i16)?;
    }
    writer.finalize()?;
    Ok(data.into_inner())
}

async fn gateway(audio: Vec<u8>) -> anyhow::Result<(String, tokio::task::JoinHandle<()>)> {
    let app = Router::new()
        .route(
            "/ari/applications/homephone",
            get(|| async { axum::Json(serde_json::json!({"channel_ids":[]})) }),
        )
        .route(
            "/ari/channels",
            get(|| async { axum::Json(Vec::<String>::new()) }),
        )
        .route(
            "/ari/bridges",
            get(|| async { axum::Json(Vec::<String>::new()) }),
        )
        .route(
            "/ari/events",
            get(|socket: axum::extract::WebSocketUpgrade| async {
                socket
                    .on_upgrade(|mut socket| async move { while socket.recv().await.is_some() {} })
            }),
        )
        .route(
            "/ari/recordings/stored/{name}/file",
            get(move || {
                let audio = audio.clone();
                async move { audio }
            }),
        )
        .fallback(any(|| async { StatusCode::NO_CONTENT }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("ARI fixture failed")
    });
    Ok((format!("http://{address}/ari/"), task))
}

async fn prepare(
    root: &Path,
    share: &Path,
) -> anyhow::Result<(
    Server,
    Config,
    Secrets,
    Vec<u8>,
    tokio::task::JoinHandle<()>,
)> {
    let share = share.to_path_buf();
    let state = root.join("samba");
    let data = root.join("data");
    let spool = root.join("spool");
    for directory in [&share, &state, &data, &spool] {
        tokio::fs::create_dir_all(directory).await?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755))?;
        std::fs::set_permissions(&share, std::fs::Permissions::from_mode(0o777))?;
    }
    let configuration = state.join("smb.conf");
    tokio::fs::write(state.join("users.map"), "recorder = \"recorder fixture\"\n").await?;
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = reservation.local_addr()?.port();
    let contents = format!(
        "[global]\nserver role = standalone server\ninterfaces = 127.0.0.1\nbind interfaces only = yes\nsmb ports = {port}\nsecurity = user\nmap to guest = Never\nusername map = {0}/users.map\nserver min protocol = SMB3\nsmb encrypt = required\npid directory = {0}\nlock directory = {0}\nstate directory = {0}\ncache directory = {0}\nprivate dir = {0}\nlog file = {0}/log\n[home]\npath = {1}\nread only = no\nvalid users = recorder\n",
        state.display(),
        share.display()
    );
    tokio::fs::write(&configuration, contents).await?;
    let password = format!(" {}-日本語-é-'\"=#$ ", Uuid::new_v4().simple());
    let mut command = Command::new("/usr/bin/smbpasswd");
    command
        .arg("-c")
        .arg(&configuration)
        .args(["-a", "-s", "recorder"])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let mut input = child
        .stdin
        .take()
        .context("missing synthetic credential pipe")?;
    input
        .write_all(format!("{password}\n{password}\n").as_bytes())
        .await?;
    drop(input);
    ensure!(
        child.wait().await?.success(),
        "SMB synthetic account initialization failed"
    );
    let audio = wav()?;
    let (ari, task) = gateway(audio.clone()).await?;
    let mut config = Config {
        data_dir: data,
        ..Config::default()
    };
    config.ai.provider = "disabled".into();
    config.ari.url = ari;
    config.storage.retention_days = 0;
    config.storage.recording_limit_bytes = 1048576;
    config.recording = Some(CallRecording {
        enabled: true,
        max_seconds: 2,
        gateway_spool_dir: spool,
        nas: Some(Nas {
            archive_id: config.instance_id,
            server: "127.0.0.1".into(),
            share: "home".into(),
            directory: "telephone/recordings".into(),
            port,
            username_env: "NAS_LAB_USERNAME".into(),
            password_env: "NAS_LAB_PASSWORD".into(),
        }),
    });
    config.validate()?;
    let secrets = Secrets::from_lookup(
        &config,
        |name| match name {
            "NAS_LAB_USERNAME" => Some("recorder fixture".into()),
            "NAS_LAB_PASSWORD" => Some(password.clone()),
            "HOMEPHONE_API_TOKEN" => Some("a".repeat(32)),
            _ => None,
        },
        None,
    )?;
    drop(reservation);
    Ok((
        Server {
            config: configuration,
            port,
            child: None,
        },
        config,
        secrets,
        audio,
        task,
    ))
}

fn passed(name: &str) {
    println!("NAS PASS {name}");
}

async fn fill_share(share: &Path) -> anyhow::Result<PathBuf> {
    let path = share.join("capacity-fixture");
    let target = path.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        for _ in 0..1024 {
            match file.write_all(&[0; 65536]) {
                Ok(()) => {}
                Err(error) if error.raw_os_error() == Some(28) => return Ok::<_, anyhow::Error>(()),
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!("NAS capacity fixture lacks its bounded share mount")
    })
    .await??;
    Ok(path)
}

async fn unprivileged_probe(root: &Path, config: &Config, secrets: &Secrets) -> anyhow::Result<()> {
    let mut client = config.clone();
    client.data_dir = root.join("runtime-client");
    tokio::fs::create_dir(&client.data_dir).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&client.data_dir, std::fs::Permissions::from_mode(0o777))?;
    }
    let path = root.join("runtime-client.toml");
    tokio::fs::write(&path, toml::to_string(&client)?).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    }
    let credentials = secrets
        .nas_credentials
        .as_ref()
        .context("NAS fixture credentials are missing")?;
    let nas = config
        .recording
        .as_ref()
        .and_then(|recording| recording.nas.as_ref())
        .context("NAS fixture configuration is missing")?;
    for correct in [true, false] {
        let mut command = Command::new(std::env::current_exe()?.with_file_name("phone-server"));
        command
            .arg("--config")
            .arg(&path)
            .arg("check-recording-storage")
            .env(&nas.username_env, &credentials.username)
            .env(
                &nas.password_env,
                if correct {
                    credentials.password.as_str()
                } else {
                    "wrong-synthetic-password"
                },
            )
            .stdin(Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.uid(1001).gid(1001);
        let output = tokio::time::timeout(Duration::from_secs(10), command.output()).await??;
        ensure!(
            output.status.success() == correct,
            "unprivileged storage probe authentication failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    ensure!(
        !client.data_dir.join("phone.sqlite").exists(),
        "storage probe opened the call service database"
    );
    ensure!(
        tokio::fs::read_dir(client.data_dir.join("recording-cache"))
            .await?
            .next_entry()
            .await?
            .is_none(),
        "storage probe retained temporary audio"
    );
    Ok(())
}

async fn interrupted_fetch(
    config: &Config,
    secrets: &Secrets,
    server: &Server,
) -> anyhow::Result<()> {
    #[cfg(not(target_os = "linux"))]
    let _ = server;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let credentials = secrets
        .nas_credentials
        .as_ref()
        .context("missing synthetic NAS credentials")?;
    let destination = phone_plugin_nas::Destination::new(
        "127.0.0.1".into(),
        "home".into(),
        "telephone".into(),
        port,
        credentials.username.clone(),
        credentials.password.clone(),
        36096,
    )?;
    let expected = phone_plugin_nas::Identity::parse(64, &"1".repeat(64))?;
    let audio = TemporaryAudio::create(&config.data_dir, None).await?;
    let path = audio.path().to_owned();
    let task = tokio::spawn(async move {
        destination
            .fetch(Uuid::new_v4(), audio.path(), &expected)
            .await
    });
    let (socket, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept()).await??;
    task.abort();
    ensure!(
        task.await.is_err_and(|error| error.is_cancelled()),
        "cancelled SMB fetch completed"
    );
    drop(socket);
    ensure!(
        !path.exists(),
        "interrupted SMB fetch left a temporary file"
    );
    #[cfg(target_os = "linux")]
    {
        let allowed = server
            .child
            .as_ref()
            .and_then(Child::id)
            .context("SMB fixture PID is absent")?
            .to_string();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut threads = tokio::fs::read_dir("/proc/self/task").await?;
                let mut extra = false;
                while let Some(thread) = threads.next_entry().await? {
                    match tokio::fs::read_to_string(thread.path().join("children")).await {
                        Ok(contents) => {
                            extra |= contents.split_whitespace().any(|pid| pid != allowed)
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                if !extra {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await??;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ensure!(
        cfg!(target_os = "linux") && Path::new("/.dockerenv").is_file(),
        "NAS lab requires its isolated Linux container"
    );
    ensure!(
        std::env::var_os("HOMEPHONE_CONFIG").is_none(),
        "NAS lab refuses an installation configuration"
    );
    let root =
        Directory(std::env::temp_dir().join(format!("homephone-nas-lab-{}", Uuid::new_v4())));
    tokio::fs::create_dir(&root.0).await?;
    let share = Directory(PathBuf::from("/nas-share").join(Uuid::new_v4().to_string()));
    tokio::fs::create_dir(&share.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            std::fs::metadata(&root.0)?.dev() != std::fs::metadata(&share.0)?.dev(),
            "NAS capacity fixture shares the local spool filesystem"
        );
    }
    tokio::fs::create_dir_all("/run/samba").await?;
    let (mut server, config, secrets, audio, ari_task) = prepare(&root.0, &share.0).await?;
    let mut system = System::open(&config, &secrets).await?;
    ensure!(
        phone_server::recordings::probe(&config, secrets.nas_credentials.as_ref())
            .await
            .is_err(),
        "offline NAS probe succeeded"
    );
    let call = system.capture().await?;
    passed("recording_capture_ai_off");
    let local = config
        .data_dir
        .join("recordings")
        .join(format!("{}.wav", call.id));
    let task = system.task(call.id).await?;
    ensure!(
        system.processor.archive_one(&task).await.is_err(),
        "unavailable NAS unexpectedly accepted audio"
    );
    ensure!(
        tokio::fs::read(&local).await? == audio,
        "NAS outage discarded local audio"
    );
    passed("nas_outage_preserves_audio");
    server.start().await?;
    phone_server::recordings::probe(&config, secrets.nas_credentials.as_ref()).await?;
    let probe = share
        .0
        .join("telephone/recordings")
        .join(
            config
                .recording
                .as_ref()
                .context("recording config missing")?
                .nas
                .as_ref()
                .context("archive config missing")?
                .archive_id
                .to_string(),
        )
        .join("00000000-0000-8000-8000-000000000001.wav");
    ensure!(!probe.exists(), "storage probe left audio on the NAS");
    passed("storage_probe_cleanup");
    unprivileged_probe(&root.0, &config, &secrets).await?;
    passed("unprivileged_storage_probe");
    system.processor.archive_one(&task).await?;
    let remote = share
        .0
        .join("telephone/recordings")
        .join(config.instance_id.to_string())
        .join(format!("{}.wav", call.id));
    ensure!(
        !local.exists() && tokio::fs::read(&remote).await? == audio,
        "verified NAS transfer did not release its source"
    );
    system.processor.archive_one(&task).await?;
    passed("verified_archive_releases_local");
    tokio::fs::write(&local, &audio).await?;
    system.close().await;
    system = System::open(&config, &secrets).await?;
    system.processor.release_archived().await?;
    ensure!(
        system.store.archived_recording(call.id).await?.is_some(),
        "archive receipt did not survive restart"
    );
    ensure!(
        !local.exists(),
        "archive recovery left a committed local copy; paused={}",
        system.store.background_paused()
    );
    passed("archive_restart_cleanup");
    system.close().await;
    let mut restored = config.clone();
    restored.instance_id = Uuid::new_v4();
    system = System::open(&restored, &secrets).await?;
    ensure!(
        system.request(call.id, "GET", Some("bytes=0-43")).await?.0 == StatusCode::PARTIAL_CONTENT,
        "restored installation lost its archived audio"
    );
    system.close().await;
    system = System::open(&config, &secrets).await?;
    passed("restored_archive_identity");
    let first = system
        .app
        .clone()
        .oneshot(
            Request::builder()
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .uri(format!("/api/calls/{}/recording", call.id))
                .header("authorization", format!("Bearer {}", "a".repeat(32)))
                .header("range", "bytes=0-43")
                .body(Body::empty())?,
        )
        .await?;
    ensure!(
        first.status() == StatusCode::PARTIAL_CONTENT,
        "initial archived range was unavailable"
    );
    let (status, bytes) = system.request(call.id, "GET", Some("bytes=44-63")).await?;
    ensure!(
        status == StatusCode::PARTIAL_CONTENT && bytes == audio[44..64],
        "archived range playback returned incorrect audio"
    );
    drop(first);
    ensure!(
        tokio::fs::read_dir(config.data_dir.join("recording-cache"))
            .await?
            .next_entry()
            .await?
            .is_none(),
        "playback kept its temporary file"
    );
    passed("authenticated_range_playback");
    for corrupt in [b"corrupt".to_vec(), vec![0; 100000]] {
        tokio::fs::write(&remote, corrupt).await?;
        ensure!(
            system.request(call.id, "GET", None).await?.0 == StatusCode::SERVICE_UNAVAILABLE,
            "invalid NAS audio was playable"
        );
        ensure!(
            tokio::fs::read_dir(config.data_dir.join("recording-cache"))
                .await?
                .next_entry()
                .await?
                .is_none(),
            "invalid NAS audio exceeded temporary storage"
        );
    }
    tokio::fs::write(&remote, &audio).await?;
    passed("corrupt_and_oversized_readback");
    interrupted_fetch(&config, &secrets, &server).await?;
    passed("interrupted_readback_cleanup");
    let second = system.capture().await?;
    let second_task = system.task(second.id).await?;
    system.store.with(|database|{database.execute_batch("CREATE TRIGGER refuse_archive BEFORE UPDATE OF state ON recording_archives WHEN NEW.state='available' BEGIN SELECT RAISE(ABORT,'synthetic archive commit failure'); END;")?;Ok(())}).await?;
    ensure!(
        system.processor.archive_one(&second_task).await.is_err(),
        "failed database commit accepted archive ownership"
    );
    let second_local = config
        .data_dir
        .join("recordings")
        .join(format!("{}.wav", second.id));
    ensure!(
        second_local.exists(),
        "failed archive commit discarded its source"
    );
    system
        .store
        .with(|database| {
            database.execute_batch("DROP TRIGGER refuse_archive;")?;
            Ok(())
        })
        .await?;
    system.processor.archive_one(&second_task).await?;
    ensure!(
        !second_local.exists(),
        "existing verified NAS object could not complete after a commit failure"
    );
    passed("failed_database_commit_retries_existing_object");
    let third = system.capture().await?;
    let third_task = system.task(third.id).await?;
    let remote_directory = remote.parent().context("NAS object lacks its directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(remote_directory, std::fs::Permissions::from_mode(0o555))?;
    }
    ensure!(
        system.processor.archive_one(&third_task).await.is_err(),
        "unwritable NAS accepted audio"
    );
    ensure!(
        config
            .data_dir
            .join("recordings")
            .join(format!("{}.wav", third.id))
            .exists(),
        "unwritable NAS discarded local audio"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(remote_directory, std::fs::Permissions::from_mode(0o755))?;
    }
    system.processor.archive_one(&third_task).await?;
    passed("unwritable_archive_preserves_audio");
    let full = system.capture().await?;
    let full_task = system.task(full.id).await?;
    let filler = fill_share(&share.0).await?;
    let error = system.processor.archive_one(&full_task).await.unwrap_err();
    ensure!(
        matches!(
            error.downcast_ref::<phone_plugin_nas::Failure>(),
            Some(phone_plugin_nas::Failure::Capacity)
        ),
        "full NAS did not report the actual remote capacity failure: {error}"
    );
    let retained = config
        .data_dir
        .join("recordings")
        .join(format!("{}.wav", full.id));
    ensure!(
        tokio::fs::read(&retained).await? == audio,
        "full NAS discarded its local recording"
    );
    tokio::fs::remove_file(filler).await?;
    system.processor.archive_one(&full_task).await?;
    ensure!(
        !retained.exists()
            && tokio::fs::read(remote_directory.join(format!("{}.wav", full.id))).await? == audio,
        "NAS capacity recovery did not publish verified audio"
    );
    passed("nas_capacity_preserves_audio");
    ensure!(
        system.request(call.id, "DELETE", None).await?.0 == StatusCode::NO_CONTENT,
        "archive deletion was not accepted"
    );
    let deletion = system.task(call.id).await?;
    ensure!(
        matches!(deletion.work, ArchiveWork::Delete),
        "deletion did not leave a durable intent"
    );
    server.stop().await?;
    ensure!(
        system.processor.archive_one(&deletion).await.is_err(),
        "unavailable NAS completed deletion"
    );
    system.close().await;
    system = System::open(&config, &secrets).await?;
    server.start().await?;
    system
        .processor
        .archive_one(&system.task(call.id).await?)
        .await?;
    ensure!(
        !remote.exists() && system.store.get(call.id).await?.is_none(),
        "remote deletion did not survive restart"
    );
    ensure!(
        !system
            .store
            .commit_archive(
                call.id,
                phone_server::store::ArchivedRecording {
                    bytes: audio.len() as u64,
                    digest: "1".repeat(64)
                },
                now()
            )
            .await?,
        "deleted archive was republished"
    );
    passed("durable_remote_deletion");
    server.stop().await?;
    let mut queued = Call::offered("456".into(), "outbound".into(), now());
    queued.phase = Phase::Ended;
    queued.ended_at = Some(now());
    queued.recording = Recording::Pending;
    system.store.save(&queued, "ended", now()).await?;
    let stop = tokio_util::sync::CancellationToken::new();
    let worker = tokio::spawn(system.processor.clone().run(stop.clone()));
    let queued_local = config
        .data_dir
        .join("recordings")
        .join(format!("{}.wav", queued.id));
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let saved = system
                .store
                .get(queued.id)
                .await?
                .context("queued call disappeared")?;
            if saved.last_error.as_deref() == Some("nas_archive_unavailable") {
                ensure!(
                    tokio::fs::read(&queued_local).await? == audio,
                    "background retry discarded its local recording"
                );
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    server.start().await?;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if system.store.archived_recording(queued.id).await?.is_some() && !queued_local.exists()
            {
                let saved = system
                    .store
                    .get(queued.id)
                    .await?
                    .context("archived call disappeared")?;
                ensure!(
                    saved.last_error.is_none() && saved.transcript.is_empty(),
                    "background archive retained an error or invoked inference"
                );
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(2), worker).await??;
    passed("background_capture_and_archive_retry");
    system.close().await;
    server.stop().await?;
    ari_task.abort();
    tokio::fs::remove_dir_all(&root.0).await?;
    tokio::fs::remove_dir_all(&share.0).await?;
    println!("NAS COMPLETE 15");
    Ok(())
}
