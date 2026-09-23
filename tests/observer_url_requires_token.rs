use std::process::{Command, Output};

const OBSERVER_URL: &str = "https://observer.example.test/";
const TOKEN_REQUIRED_MESSAGE: &str =
    "--observer-api-url / OBSERVER_API_URL requires DESLICER_API_TOKEN";

struct ObserverUrlInvocation;

impl ObserverUrlInvocation {
    fn with_flag() -> Output {
        Self::command()
            .arg("--observer-api-url")
            .arg(OBSERVER_URL)
            .output()
            .expect("run deslicer with Observer URL flag")
    }

    fn with_environment() -> Output {
        Self::command()
            .env("OBSERVER_API_URL", OBSERVER_URL)
            .output()
            .expect("run deslicer with Observer URL environment variable")
    }

    fn command() -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_deslicer"));
        command
            .arg("auth")
            .arg("status")
            .arg("--ci-platform")
            .arg("local")
            .env_remove("OBSERVER_API_URL")
            .env_remove("DESLICER_API_TOKEN")
            .env_remove("DESLICER_DEV_TOKEN");
        command
    }
}

fn assert_token_required(output: Output) {
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(TOKEN_REQUIRED_MESSAGE),
        "expected token requirement, got: {stderr}"
    );
    assert!(
        !stderr.contains("not logged in") && !stderr.contains("OIDC"),
        "must not fall through to unrelated authentication: {stderr}"
    );
}

#[test]
fn observer_url_flag_without_token_fails_clearly() {
    assert_token_required(ObserverUrlInvocation::with_flag());
}

#[test]
fn observer_url_environment_without_token_fails_clearly() {
    assert_token_required(ObserverUrlInvocation::with_environment());
}
