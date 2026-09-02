#![allow(clippy::await_holding_lock)]

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::commands::inventory::validate;
use deslicer_cli::Ctx;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[allow(clippy::await_holding_lock)]
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn clear_env() {
    std::env::remove_var("DESLICER_API_TOKEN");
}

fn write_app(root: &std::path::Path, rel: &str) {
    let default = root.join(rel).join("default");
    std::fs::create_dir_all(&default).expect("app dir");
    std::fs::write(default.join("app.conf"), "").expect("app.conf");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn inventory_validate_accepts_known_group() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "019f36d6-3f61-7eea-9417-7ac4a8a10f69", "name": "indexers"}
        ])))
        .mount(&observer)
        .await;

    let dir = tempdir().expect("temp");
    write_app(dir.path(), "apps/ta_nix");
    let env_dir = dir.path().join(".deslicer/environments");
    std::fs::create_dir_all(&env_dir).expect("env dir");
    std::fs::write(
        env_dir.join("acme-prod.yml"),
        "destinations:\n  - inventory_group: indexers\n    apps:\n      - source_path: apps/ta_nix\n",
    )
    .expect("seed env");

    let ctx = Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&observer.uri()).expect("observer")),
        ci_override: Some(CiPlatform::Local),
        log_format: LogFormat::Json,
    };
    let code = validate::run(
        ctx,
        validate::Args {
            environment: Some("acme-prod".into()),
            dir: Some(dir.path().to_path_buf()),
        },
    )
    .await;
    clear_env();
    assert_eq!(code, 0);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn inventory_validate_rejects_unknown_group() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "019f36d6-3f61-7eea-9417-7ac4a8a10f69", "name": "indexers"}
        ])))
        .mount(&observer)
        .await;

    let dir = tempdir().expect("temp");
    write_app(dir.path(), "apps/ta_nix");
    let env_dir = dir.path().join(".deslicer/environments");
    std::fs::create_dir_all(&env_dir).expect("env dir");
    std::fs::write(
        env_dir.join("acme-prod.yml"),
        "destinations:\n  - inventory_group: ghost\n    apps:\n      - source_path: apps/ta_nix\n",
    )
    .expect("seed env");

    let ctx = Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&observer.uri()).expect("observer")),
        ci_override: Some(CiPlatform::Local),
        log_format: LogFormat::Human,
    };
    let code = validate::run(
        ctx,
        validate::Args {
            environment: Some("acme-prod".into()),
            dir: Some(dir.path().to_path_buf()),
        },
    )
    .await;
    clear_env();
    assert_eq!(code, 1);
}
