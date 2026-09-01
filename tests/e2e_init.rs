use base64::Engine;
use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::commands::init::{self, InitProvider};
use deslicer_cli::Ctx;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

#[tokio::test]
async fn init_github_token_writes_path_a2_workflow() {
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");

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
        environment: None,
        target_group: None,
        dir: Some(dir.path().to_path_buf()),
        bind: false,
        offline: false,
        force: false,
    };
    let code = init::run(ctx, args).await;
    std::env::remove_var("DESLICER_API_TOKEN");
    assert_eq!(code, 0);

    let written =
        std::fs::read_to_string(dir.path().join(".github/workflows/deslicer-plan.yml"))
            .expect("workflow");
    assert!(written.contains("--target-group"));
    assert!(!written.contains("id-token: write"));
    assert!(written.contains("contents: read"));
    assert_eq!(
        InitProvider::parse("github-token").unwrap(),
        InitProvider::GithubToken
    );
}

#[tokio::test]
async fn init_refuses_sha_mismatch() {
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");

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
    std::env::remove_var("DESLICER_API_TOKEN");
    assert_eq!(code, 1);
}
