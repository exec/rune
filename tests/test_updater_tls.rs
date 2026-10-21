//! Explicit network checks for TLS dependency updates; excluded from offline CI.
#[test]
#[ignore = "requires access to the GitHub release API"]
fn updater_fetches_over_verified_tls() {
    let cache = tempfile::tempdir().unwrap();
    let latest = rune::updater::check_with_cache_dir(cache.path(), "0.0.0");
    assert!(latest.is_some(), "live release check failed");
    assert!(cache.path().join("update.json").exists());
}

#[test]
#[ignore = "requires access to badssl.com"]
fn updater_tls_dependencies_reject_untrusted_certificates() {
    let error = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .get("https://self-signed.badssl.com/")
        .call()
        .unwrap_err();
    assert!(error.to_string().contains("certificate"), "{error}");
}
