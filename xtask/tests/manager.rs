mod support;

use xtask::manager::{Image, Release, compatible, systemd_unit, verify_identity};

#[test]
fn a_pending_recovery_journal_refuses_backup_before_reading_installation_configuration() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap().join("installation");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(
        directory
            .parent()
            .unwrap()
            .join(".installation.operation.json"),
        "pending recovery",
    )
    .unwrap();
    let error = xtask::manager::backup(&directory, &root.path().join("backup")).unwrap_err();
    assert!(
        format!("{error:#}").contains("pending recovery journal"),
        "{error:#}"
    );
    assert!(!root.path().join("backup").exists());
}

#[test]
fn recovery_retires_rollback_before_resuming_and_failed_retirement_never_sends_http() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap().join("installation");
    std::fs::create_dir_all(directory.join("config")).unwrap();
    std::fs::create_dir(directory.join("run")).unwrap();
    let marker = directory.join("run/homephone-maintenance");
    std::fs::write(&marker, "paused").unwrap();
    let journal = directory
        .parent()
        .unwrap()
        .join(".installation.operation.json");
    std::fs::write(&journal, "uncommitted rollback instruction").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::fs::write(
        directory.join("config/homephone.toml"),
        format!(
            "instance_id = '{}'\nlisten = '{address}'\ndata_dir = '{}'\n",
            uuid::Uuid::new_v4(),
            directory.join("data").display()
        ),
    )
    .unwrap();
    std::fs::write(
        directory.join("config/secrets.env"),
        format!("HOMEPHONE_API_TOKEN={}\n", "a".repeat(32)),
    )
    .unwrap();
    std::fs::write(
        directory.join("config/images.json"),
        serde_json::to_vec(&Release {
            schema: 1,
            source: "a".repeat(64),
            architecture: "arm64".into(),
            app: Image {
                reference: "app:pinned".into(),
                fingerprint: "b".repeat(64),
            },
            gateway: Image {
                reference: "gateway:pinned".into(),
                fingerprint: "c".repeat(64),
            },
        })
        .unwrap(),
    )
    .unwrap();
    let observed = journal.clone();
    let server_listener = listener.try_clone().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = server_listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        support::read_request(&mut socket);
        let retired = !observed.exists();
        write!(
            socket,
            "HTTP/1.1 204 Result\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        retired
    });
    let installation = xtask::manager::Installation::open(&directory).unwrap();
    installation.resume().unwrap();
    assert!(
        server.join().unwrap(),
        "rollback was still armed when call admission resumed"
    );
    listener.set_nonblocking(true).unwrap();
    std::fs::create_dir(&journal).unwrap();
    std::fs::write(&marker, "paused").unwrap();
    assert!(installation.resume().is_err());
    assert!(marker.is_file());
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn rebuilding_a_source_cannot_replace_the_previous_deployment_image_reference() {
    let source = "a".repeat(64);
    let first = xtask::manager::image_references(&source, uuid::Uuid::from_u128(1)).unwrap();
    let second = xtask::manager::image_references(&source, uuid::Uuid::from_u128(2)).unwrap();
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
    assert!(xtask::manager::image_references("invalid", uuid::Uuid::nil()).is_err());
}

#[test]
fn administrative_paths_resolve_existing_aliases_and_refuse_parent_traversal() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("installation");
    std::fs::create_dir(&nested).unwrap();
    assert!(xtask::manager::absolute(&nested.join("../installation/backup")).is_err());
    assert_eq!(
        xtask::manager::absolute(&nested.join("./backup")).unwrap(),
        nested.canonicalize().unwrap().join("backup")
    );
    #[cfg(unix)]
    {
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&nested, &alias).unwrap();
        assert_eq!(
            xtask::manager::absolute(&alias.join("backup")).unwrap(),
            nested.canonicalize().unwrap().join("backup")
        );
    }
}

#[test]
fn quiescence_is_persisted_before_http_and_refusal_removes_only_a_new_marker() {
    use std::io::Write;
    for (status, existing) in [(204, false), (409, false), (409, true)] {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().canonicalize().unwrap();
        std::fs::create_dir(directory.join("config")).unwrap();
        std::fs::create_dir(directory.join("run")).unwrap();
        let marker = directory.join("run/homephone-maintenance");
        if existing {
            std::fs::write(&marker, b"paused").unwrap();
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let observed_marker = marker.clone();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            support::read_request(&mut socket);
            let observed = observed_marker.is_file();
            write!(
                socket,
                "HTTP/1.1 {status} Result\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            observed
        });
        std::fs::write(directory.join("config/homephone.toml"),format!("instance_id = '{}'\nlisten = '{address}'\ndata_dir = '{}'\napi_token_env = 'HOMEPHONE_API_TOKEN'\n",uuid::Uuid::new_v4(),directory.join("data").display())).unwrap();
        std::fs::write(
            directory.join("config/secrets.env"),
            format!("HOMEPHONE_API_TOKEN={}\n", "a".repeat(32)),
        )
        .unwrap();
        std::fs::write(
            directory.join("config/images.json"),
            serde_json::to_vec(&Release {
                schema: 1,
                source: "a".repeat(64),
                architecture: "arm64".into(),
                app: Image {
                    reference: "app:pinned".into(),
                    fingerprint: "b".repeat(64),
                },
                gateway: Image {
                    reference: "gateway:pinned".into(),
                    fingerprint: "c".repeat(64),
                },
            })
            .unwrap(),
        )
        .unwrap();
        let installation = xtask::manager::Installation::open(&directory).unwrap();
        let result = installation.maintenance(true);
        assert_eq!(result.is_ok(), status == 204, "{result:?}");
        assert!(
            server.join().unwrap(),
            "quiescence was not durable before the request"
        );
        assert_eq!(marker.is_file(), existing || status == 204);
    }
}

#[test]
fn deployment_requires_arm64_immutable_images_and_the_exact_service_identity() {
    let image = Image {
        reference: "homephone:source-abc".into(),
        fingerprint: "a".repeat(64),
    };
    let release = Release {
        schema: 1,
        source: "b".repeat(64),
        architecture: "arm64".into(),
        app: image.clone(),
        gateway: image,
    };
    compatible(&release, "arm64").unwrap();
    assert!(compatible(&release, "amd64").is_err());
    let mut malformed = release.clone();
    malformed.app.fingerprint = "latest".into();
    assert!(compatible(&malformed, "arm64").is_err());
    let id = uuid::Uuid::new_v4();
    verify_identity(&serde_json::json!({"instance_id":id}), id).unwrap();
    assert!(verify_identity(&serde_json::json!({"instance_id":uuid::Uuid::new_v4()}), id).is_err());
    assert!(verify_identity(&serde_json::json!({"healthy":true}), id).is_err());
}

#[test]
fn boot_units_preserve_paths_and_use_the_supervised_lifecycle() {
    let unit = systemd_unit(
        std::path::Path::new("/opt/home phone/homephone"),
        std::path::Path::new("/srv/home phone"),
        "1000",
    )
    .unwrap();
    assert!(unit.contains("Requires=docker.service"));
    assert!(unit.contains("After=network-online.target docker.service"));
    assert!(
        unit.contains(
            "ExecStart=\"/opt/home phone/homephone\" boot --directory \"/srv/home phone\""
        )
    );
    assert!(
        unit.contains(
            "ExecStop=\"/opt/home phone/homephone\" stop --directory \"/srv/home phone\""
        )
    );
    assert!(
        systemd_unit(
            std::path::Path::new("/opt/a\nb"),
            std::path::Path::new("/srv/phone"),
            "root"
        )
        .is_err()
    );
}
