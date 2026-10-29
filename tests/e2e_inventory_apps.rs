//! `inventory apps [--host]` and `inventory apps adopt --yes` against a mocked
//! Observer using the real `insights/apps` and `app-provenance/adopt` shapes.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const IDX1: &str = "11111111-1111-4111-8111-111111111111";
const SH1: &str = "22222222-2222-4222-8222-222222222222";

async fn run(dir: &Path, observer: &str, args: &[&str]) -> Output {
    let dir = dir.to_path_buf();
    let observer = observer.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        Command::new(env!("CARGO_BIN_EXE_deslicer"))
            .env_clear()
            .env("HOME", &home)
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("OBSERVER_API_URL", format!("{observer}/"))
            .env("DESLICER_API_TOKEN", "dap_tools_test_key")
            .current_dir(&dir)
            .args(&args)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

async fn mount_apps(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/insights/apps"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "apps": [{
                "app_name": "TA-nix",
                "installed_on": [{"host_id": IDX1, "hostname": "idx1", "observed_version": "9.0.0"}],
                "missing_from": [{"host_id": SH1, "hostname": "sh1"}],
                "total_enrolled": 2, "coverage_pct": 50.0, "stanza_count": 3,
                "last_seen": "2026-10-29T08:00:00Z"
            }],
            "total_enrolled_hosts": 2, "total": 1, "limit": 2000, "offset": 0
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn list_filters_by_hostname_and_rejects_unknown_host() {
    let observer = MockServer::start().await;
    mount_apps(&observer).await;
    let dir = tempfile::tempdir().unwrap();

    let out = run(
        dir.path(),
        &observer.uri(),
        &[
            "--log-format",
            "json",
            "inventory",
            "apps",
            "--host",
            "idx1",
        ],
    )
    .await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows[0]["name"], "TA-nix");
    assert_eq!(rows[0]["host_id"], IDX1);
    assert_eq!(rows[0]["version"], "9.0.0");

    let out = run(
        dir.path(),
        &observer.uri(),
        &["inventory", "apps", "--host", "nope"],
    )
    .await;
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test]
async fn adopt_sends_git_managed_body_with_host_id() {
    let observer = MockServer::start().await;
    mount_apps(&observer).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/app-provenance/adopt"))
        .and(body_json(json!({
            "scope": "apps",
            "app_name": "TA-nix",
            "host_id": IDX1,
            "management_mode": "managed",
            "content_source": "git_tree",
            "source_path": "apps/TA-nix"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "33333333-3333-4333-8333-333333333333",
            "inventory_group_id": null, "scope": "apps", "app_name": "TA-nix",
            "host_id": IDX1, "management_mode": "managed", "content_source": "git_tree",
            "source_path": "apps/TA-nix"
        })))
        .expect(1)
        .mount(&observer)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        &observer.uri(),
        &[
            "inventory",
            "apps",
            "adopt",
            "TA-nix",
            "--host",
            "idx1",
            "--source-path",
            "apps/TA-nix",
            "--yes",
        ],
    )
    .await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("adopted TA-nix on idx1 as managed"));
    assert!(!stderr.contains("dap_tools_test_key"));

    // Not installed on sh1: exit 2, no adopt call (expect(1) above).
    let out = run(
        dir.path(),
        &observer.uri(),
        &[
            "inventory",
            "apps",
            "adopt",
            "TA-nix",
            "--host",
            "sh1",
            "--yes",
        ],
    )
    .await;
    assert_eq!(out.status.code(), Some(2));
}
