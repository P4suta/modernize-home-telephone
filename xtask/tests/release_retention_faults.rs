use xtask::manager::{Image, Release, image_references, superseded_references};

fn release(source: char, build: u128) -> Release {
    let source = source.to_string().repeat(64);
    let (app, gateway) = image_references(&source, uuid::Uuid::from_u128(build)).unwrap();
    let fingerprint = "f".repeat(64);
    Release {
        schema: 2,
        source,
        architecture: "arm64".into(),
        app: Image {
            reference: app,
            fingerprint: fingerprint.clone(),
        },
        gateway: Image {
            reference: gateway,
            fingerprint,
        },
    }
}

#[test]
fn committed_replacements_select_only_superseded_release_images_that_no_container_uses() {
    let running = release('a', 1);
    let previous = release('b', 2);
    let older = release('c', 3);
    let in_use = release('d', 4);
    let listed = vec![
        format!("docker.io/library/{}", running.app.reference),
        running.gateway.reference.clone(),
        format!("localhost/{}", previous.app.reference),
        format!("docker.io/library/{}", previous.gateway.reference),
        format!("docker.io/library/{}", older.app.reference),
        older.gateway.reference.clone(),
        format!("docker.io/library/{}", in_use.app.reference),
        "docker.io/binwiederhier/ntfy:v2".into(),
        "homephone-app:development".into(),
        "registry.example/homephone-app:source-0000".into(),
        "<none>:<none>".into(),
    ];
    let containers = vec![in_use.app.reference.clone()];
    assert_eq!(
        superseded_references(&listed, &containers, &[&running, &previous]),
        vec![
            format!("docker.io/library/{}", older.app.reference),
            older.gateway.reference.clone(),
        ]
    );
    assert!(superseded_references(&listed, &containers, &[&running, &previous, &older]).is_empty());
}
