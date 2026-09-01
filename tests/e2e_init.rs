use base64::Engine;
use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::commands::init::{self, InitProvider};
use deslicer_cli::Ctx;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// REQ: serialize DESLICER_* env across async e2e tests (process-global).
#[allow(clippy::await_holding_lock)]
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn mock_file(path: &str, body: &str) -> serde_json::Value {
    let bytes = body.as_bytes();
    json!({
        "path": path,
        "sha256": sha256_hex(bytes),
        "contents_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

fn clear_init_env() {
    std::env::remove_var("DESLICER_API_TOKEN");
    std::env::remove_var("DESLICER_CACHE_DIR");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn init_github_token_writes_path_a2_workflow() {
    let _guard = env_lock().lock().expect("env lock");
    let cache = tempdir().expect("cache");
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");
    std::env::set_var("DESLICER_CACHE_DIR", cache.path());

    let plan = r#"# Path A2
permissions:
  contents: read
jobs:
  plan:
    steps:
      - run: deslicer change plan --target-group "${{ vars.TARGET_GROUP_ID }}"
"#;
    let actions = "options: [deploy, status, verify]\n";
    let script = "#!/usr/bin/env bash\necho target_path\n";
    let readme = "# Path A2\n";

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "019f36d6-3f61-7eea-9417-7ac4a8a10f69", "name": "indexers"},
            {"id": "019f36d6-3f61-7eea-9417-7ac4a8a10f70", "name": "forwarders"}
        ])))
        .mount(&observer)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/bootstrap-templates"))
        .and(query_param("provider", "github-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "provider": "github-token",
            "tree_sha256": "a".repeat(64),
            "files": [
                mock_file(".github/workflows/deslicer-plan.yml", plan),
                mock_file(".github/workflows/deslicer-plan-actions.yml", actions),
                mock_file(".github/scripts/append-plan-changed-files.sh", script),
                mock_file("README.md", readme),
            ]
        })))
        .mount(&observer)
        .await;

    let dir = tempdir().expect("temp");
    let ctx = Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&observer.uri()).expect("observer")),
        ci_override: Some(CiPlatform::Local),
        log_format: LogFormat::Human,
    };
    let args = init::Args {
        provider: "github-token".into(),
        environment: Some("acme-prod".into()),
        target_group: None,
        dir: Some(dir.path().to_path_buf()),
        bind: false,
        offline: false,
        force: false,
    };
    let code = init::run(ctx, args).await;
    clear_init_env();
    assert_eq!(code, 0);

    let written = std::fs::read_to_string(dir.path().join(".github/workflows/deslicer-plan.yml"))
        .expect("workflow");
    assert!(written.contains("--target-group"));
    assert!(!written.contains("id-token: write"));
    assert!(written.contains("contents: read"));
    let env_file = std::fs::read_to_string(dir.path().join(".deslicer/environments/acme-prod.yml"))
        .expect("tenant env");
    assert!(env_file.contains("inventory_group: indexers"));
    assert!(env_file.contains("inventory_group: forwarders"));
    assert_eq!(
        InitProvider::parse("github-token").unwrap(),
        InitProvider::GithubToken
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn init_refuses_sha_mismatch() {
    let _guard = env_lock().lock().expect("env lock");
    let cache = tempdir().expect("cache");
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");
    std::env::set_var("DESLICER_CACHE_DIR", cache.path());

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/bootstrap-templates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "provider": "github-token",
            "tree_sha256": "b".repeat(64),
            "files": [{
                "path": "README.md",
                "sha256": "0".repeat(64),
                "contents_base64": base64::engine::general_purpose::STANDARD.encode("nope"),
            }]
        })))
        .mount(&observer)
        .await;

    let dir = tempdir().expect("temp");
    let ctx = Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&observer.uri()).expect("observer")),
        ci_override: Some(CiPlatform::Local),
        log_format: LogFormat::Human,
    };
    let args = init::Args {
        provider: "github-token".into(),
        environment: None,
        target_group: None,
        dir: Some(dir.path().to_path_buf()),
        bind: false,
        offline: false,
        force: false,
    };
    let code = init::run(ctx, args).await;
    clear_init_env();
    assert_eq!(code, 1);
}
