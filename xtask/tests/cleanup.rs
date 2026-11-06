#[test]
fn cleanup_preserves_the_primary_error_and_reports_secondary_failures() {
    use std::io::{Error, ErrorKind};
    assert_eq!(xtask::operations::finish_cleanup(Ok(7), Ok(())).unwrap(), 7);
    assert_eq!(
        xtask::operations::finish_cleanup(Ok(7), Err(Error::from(ErrorKind::NotFound))).unwrap(),
        7
    );
    let primary: anyhow::Result<()> = Err(anyhow::anyhow!("primary failure"));
    let combined = xtask::operations::finish_cleanup(primary, Err(Error::other("cleanup failure")))
        .unwrap_err();
    let chain = format!("{combined:#}");
    assert!(chain.contains("primary failure"));
    assert!(chain.contains("cleanup failure"));
    let failed: anyhow::Result<()> =
        xtask::operations::finish_cleanup(Ok(()), Err(Error::other("cleanup failure")));
    assert!(format!("{:#}", failed.unwrap_err()).contains("cleanup failure"));
}
