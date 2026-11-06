#![cfg(feature = "upstream-archives")]

#[path = "support/packages.rs"]
mod packages;

#[test]
fn prefetched_packages_do_not_require_repository_downloads() {
    assert_eq!(packages::verify().unwrap(), 5);
}
