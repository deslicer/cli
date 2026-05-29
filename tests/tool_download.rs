// ENV_LOCK only serializes env access across single-threaded tests;
// holding it across the await is safe (no cross-task contention).
#![allow(clippy::await_holding_lock)]

use deslicer_cli::errors::CliError;
use deslicer_cli::tool_download::download_and_verify;
use std::sync::Mutex;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const FAKE_BINARY: &[u8] = b"fake-binary";
const FAKE_SHA256: &str = "d551e7e4f06f7d6bc7f9ad6828eb6e00e780dbe60a8adda40186c432495d0b24";

#[tokio::test]
async fn download_verify_and_cache_write() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cache_root = tempfile::tempdir().unwrap();
    std::env::set_var("DESLICER_CACHE_DIR", cache_root.path());

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tools/download"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_BINARY))
        .expect(1)
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    let cached = download_and_verify(&base, "test-token", FAKE_SHA256)
        .await
        .unwrap();

    assert!(cached.exists());
    assert_eq!(tokio::fs::read(&cached).await.unwrap(), FAKE_BINARY);
    assert_eq!(
        cached,
        cache_root
            .path()
            .join(format!("tenant-config-processor-{FAKE_SHA256}"))
    );

    std::env::remove_var("DESLICER_CACHE_DIR");
}

#[tokio::test]
async fn cache_hit_skips_second_download() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cache_root = tempfile::tempdir().unwrap();
    std::env::set_var("DESLICER_CACHE_DIR", cache_root.path());

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tools/download"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_BINARY))
        .expect(1)
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();

    let first = download_and_verify(&base, "test-token", FAKE_SHA256)
        .await
        .unwrap();
    let second = download_and_verify(&base, "test-token", FAKE_SHA256)
        .await
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(tokio::fs::read(&second).await.unwrap(), FAKE_BINARY);

    std::env::remove_var("DESLICER_CACHE_DIR");
}

#[tokio::test]
async fn checksum_mismatch_does_not_write_cache() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cache_root = tempfile::tempdir().unwrap();
    std::env::set_var("DESLICER_CACHE_DIR", cache_root.path());

    let wrong_sha = "0000000000000000000000000000000000000000000000000000000000000000";

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tools/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FAKE_BINARY))
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    let err = download_and_verify(&base, "test-token", wrong_sha)
        .await
        .unwrap_err();

    assert!(matches!(err, CliError::Other(_)));
    assert!(
        err.to_string().contains("checksum mismatch"),
        "unexpected error: {err}"
    );

    let cache_file = cache_root
        .path()
        .join(format!("tenant-config-processor-{wrong_sha}"));
    assert!(!cache_file.exists());

    std::env::remove_var("DESLICER_CACHE_DIR");
}
