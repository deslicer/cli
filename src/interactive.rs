use std::io::IsTerminal;

/// True when the CLI must not block on user input or portal polling.
///
/// CI runners and piped/non-TTY invocations should fail fast or emit a device
/// code and exit instead of waiting for approval.
pub fn is_non_interactive() -> bool {
    if std::env::var_os("CI").is_some_and(|value| !value.is_empty() && value != "0") {
        return true;
    }
    !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn ci_env_forces_non_interactive() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("CI", "1");
        assert!(is_non_interactive());
        std::env::remove_var("CI");
    }

    #[test]
    fn ci_zero_is_not_forced_non_interactive() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("CI", "0");
        let _ = is_non_interactive();
        std::env::remove_var("CI");
    }
}
