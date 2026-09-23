use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const LEGACY_TOKEN: &str = "legacy-secret-must-not-leak";

fn cli_command(config_dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    command
        .env("DESLICER_CONFIG_DIR", config_dir)
        .env("DESLICER_TOKEN_STORE", "file")
        .env_remove("OBSERVER_API_URL")
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("GITLAB_CI")
        .env_remove("TF_BUILD")
        .env_remove("BITBUCKET_BUILD_NUMBER");
    command
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_retirement_error(output: &Output) {
    assert!(!output.status.success());
    let text = combined_output(output);
    assert!(text.contains("DESLICER_DEV_TOKEN has been retired"));
    assert!(text.contains("OBSERVER_API_URL"));
    assert!(text.contains("DESLICER_API_TOKEN"));
    assert!(text.contains("deslicer auth login"));
    assert!(!text.contains(LEGACY_TOKEN));
}

#[tokio::test]
async fn legacy_token_fails_consistently_without_network_transmission() {
    let server = MockServer::start().await;
    let config = TempDir::new().unwrap();
    let api_url = format!("{}/", server.uri());
    let commands: &[&[&str]] = &[
        &["auth", "login"],
        &["auth", "status"],
        &["auth", "whoami"],
        &["groups", "list"],
    ];

    for args in commands {
        let output = cli_command(config.path())
            .args(*args)
            .args(["--deslicer-api-url", &api_url])
            .env("DESLICER_DEV_TOKEN", LEGACY_TOKEN)
            .env("CI", "1")
            .output()
            .unwrap();
        assert_retirement_error(&output);
    }

    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn direct_observer_credentials_take_precedence_over_legacy_token() {
    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .and(header("Authorization", "Bearer tools-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&observer)
        .await;
    let config = TempDir::new().unwrap();
    write_device_session(config.path());
    let observer_url = format!("{}/", observer.uri());

    for args in [
        &["auth", "login"][..],
        &["auth", "status"][..],
        &["auth", "whoami"][..],
        &["groups", "list"][..],
    ] {
        let output = cli_command(config.path())
            .args(args)
            .env("OBSERVER_API_URL", &observer_url)
            .env("DESLICER_API_TOKEN", "tools-key")
            .env("DESLICER_DEV_TOKEN", LEGACY_TOKEN)
            .env("GITHUB_ACTIONS", "true")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "command failed: {}",
            combined_output(&output)
        );
        let text = combined_output(&output);
        if args.first() == Some(&"auth") {
            assert!(text.contains("observer_api_token"));
        }
        assert!(!text.contains(LEGACY_TOKEN));
    }
}

#[test]
fn active_device_session_takes_precedence_over_legacy_token() {
    let config = TempDir::new().unwrap();
    write_device_session(config.path());

    for args in [
        &["auth", "login"][..],
        &["auth", "status"][..],
        &["auth", "whoami"][..],
    ] {
        let output = cli_command(config.path())
            .args(args)
            .env("DESLICER_DEV_TOKEN", LEGACY_TOKEN)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "command failed: {}",
            combined_output(&output)
        );
        assert!(combined_output(&output).contains("device"));
        assert!(!combined_output(&output).contains(LEGACY_TOKEN));
    }
}

#[tokio::test]
async fn real_ci_oidc_takes_precedence_over_legacy_token() {
    let oidc = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/oidc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "value": "real-ci-jwt"
        })))
        .mount(&oidc)
        .await;
    let config = TempDir::new().unwrap();

    let output = cli_command(config.path())
        .args(["auth", "whoami", "--log-format", "json"])
        .env("GITHUB_ACTIONS", "true")
        .env(
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            format!("{}/oidc?request=1", oidc.uri()),
        )
        .env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "runner-request-token")
        .env("DESLICER_DEV_TOKEN", LEGACY_TOKEN)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", combined_output(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"identity\": \"ci\""));
    assert!(!combined_output(&output).contains(LEGACY_TOKEN));
}

fn write_device_session(config_dir: &Path) {
    fs::create_dir_all(config_dir).unwrap();
    fs::write(
        config_dir.join("credentials.toml"),
        r#"cli_session_token = "dslcli_test"
expires_at = "2099-01-01T00:00:00.000Z"
tenant_id = "tenant-test"
display_name = "Test User"
observer_api_url = "https://api.deslicer.ai/api/cli/observer/"
tenant_slug = "test"
deslicer_api_url = "https://api.deslicer.ai/"
"#,
    )
    .unwrap();
}
