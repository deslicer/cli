#![allow(clippy::await_holding_lock)]

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::commands::inventory::sync;
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

fn clear_sync_env() {
    std::env::remove_var("DESLICER_API_TOKEN");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn inventory_sync_writes_tenant_environment_file() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "019f36d6-3f61-7eea-9417-7ac4a8a10f69", "name": "indexers"},
            {"id": "019f36d6-3f61-7eea-9417-7ac4a8a10f70", "name": "search_heads"}
        ])))
        .mount(&observer)
        .await;

    let dir = tempdir().expect("temp");
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
        log_format: LogFormat::Human,
    };
    let args = sync::Args {
        environment: Some("acme-prod".into()),
        dir: Some(dir.path().to_path_buf()),
        dry_run: false,
    };
    let code = sync::run(ctx, args).await;
    clear_sync_env();
    assert_eq!(code, 0);

    let written = std::fs::read_to_string(env_dir.join("acme-prod.yml")).expect("env");
    assert!(written.contains("source_path: apps/ta_nix"));
    assert!(written.contains("inventory_group: search_heads"));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn inventory_sync_exits_2_when_removed_group_still_has_apps() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "019f36d6-3f61-7eea-9417-7ac4a8a10f70", "name": "forwarders"}
        ])))
        .mount(&observer)
        .await;

    let dir = tempdir().expect("temp");
    let env_dir = dir.path().join(".deslicer/environments");
    std::fs::create_dir_all(&env_dir).expect("env dir");
    std::fs::write(
        env_dir.join("acme-prod.yml"),
        "destinations:\n  - inventory_group: indexers\n    apps:\n      - source_path: apps/ta_nix\n  - inventory_group: forwarders\n    apps:\n",
    )
    .expect("seed env");

    let ctx = Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&observer.uri()).expect("observer")),
        ci_override: Some(CiPlatform::Local),
        log_format: LogFormat::Human,
    };
    let args = sync::Args {
        environment: Some("acme-prod".into()),
        dir: Some(dir.path().to_path_buf()),
        dry_run: false,
    };
    let code = sync::run(ctx, args).await;
    clear_sync_env();
    assert_eq!(code, 2);

    let written = std::fs::read_to_string(env_dir.join("acme-prod.yml")).expect("env");
    assert!(written.contains("source_path: apps/ta_nix"));
    assert!(written.contains("inventory_group: indexers"));
}
