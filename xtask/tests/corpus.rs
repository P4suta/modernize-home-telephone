#[test]
fn replayable_parser_seeds_cover_admitted_numbers_paths_and_refusals() {
    let root = tempfile::tempdir().unwrap();
    xtask::assurance::write_corpora(root.path()).unwrap();
    let inputs: Vec<Vec<u8>> = std::fs::read_dir(root.path().join("dial_path"))
        .unwrap()
        .map(|entry| std::fs::read(entry.unwrap().path()).unwrap())
        .collect();
    assert!(
        inputs
            .iter()
            .any(|input| phone_core::DialNumber::parse(input).is_ok())
    );
    assert!(
        inputs
            .iter()
            .any(|input| phone_core::DialNumber::parse(input).is_err())
    );
    assert!(
        inputs
            .iter()
            .any(|input| phone_core::artifact::RelativePath::parse(input).is_some())
    );
    assert!(
        inputs
            .iter()
            .any(|input| phone_core::artifact::RelativePath::parse(input).is_none())
    );
    assert!(
        std::fs::read_dir(root.path().join("state_sequences"))
            .unwrap()
            .next()
            .is_some()
    );
}
