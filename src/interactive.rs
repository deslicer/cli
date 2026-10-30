use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

/// Process-wide latch set from the global `--no-input` / `--non-interactive` flag.
static FORCE_NO_INPUT: AtomicBool = AtomicBool::new(false);

/// Record the parsed global `--no-input` / `--non-interactive` flag for the process.
pub fn set_no_input(value: bool) {
    FORCE_NO_INPUT.store(value, Ordering::Relaxed);
}

fn env_truthy(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| {
        let raw = value.to_string_lossy();
        let trimmed = raw.trim();
        !trimmed.is_empty() && trimmed != "0" && !trimmed.eq_ignore_ascii_case("false")
    })
}

/// True when `--no-input` / `--non-interactive`, `DESLICER_NO_INPUT`, or `CI`
/// forced non-interactive mode (excludes bare non-TTY detection).
pub fn is_forced_no_input() -> bool {
    FORCE_NO_INPUT.load(Ordering::Relaxed) || env_truthy("DESLICER_NO_INPUT") || env_truthy("CI")
}

/// True when the CLI must not block on user input or open a browser.
///
/// Forced by `--no-input` / `--non-interactive`, `DESLICER_NO_INPUT`, `CI`, or
/// a non-TTY stdin/stdout.
pub fn is_non_interactive() -> bool {
    if is_forced_no_input() {
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
        let prev = FORCE_NO_INPUT.swap(false, Ordering::Relaxed);
        std::env::set_var("CI", "1");
        std::env::remove_var("DESLICER_NO_INPUT");
        assert!(is_non_interactive());
        std::env::remove_var("CI");
        FORCE_NO_INPUT.store(prev, Ordering::Relaxed);
    }

    #[test]
    fn ci_zero_is_not_forced_non_interactive() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev = FORCE_NO_INPUT.swap(false, Ordering::Relaxed);
        std::env::set_var("CI", "0");
        std::env::remove_var("DESLICER_NO_INPUT");
        let _ = is_non_interactive();
        std::env::remove_var("CI");
        FORCE_NO_INPUT.store(prev, Ordering::Relaxed);
    }

    #[test]
    fn deslicer_no_input_env_forces_non_interactive() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev = FORCE_NO_INPUT.swap(false, Ordering::Relaxed);
        std::env::remove_var("CI");
        std::env::set_var("DESLICER_NO_INPUT", "1");
        assert!(is_non_interactive());
        std::env::remove_var("DESLICER_NO_INPUT");
        FORCE_NO_INPUT.store(prev, Ordering::Relaxed);
    }

    #[test]
    fn set_no_input_forces_non_interactive() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev_ci = std::env::var_os("CI");
        let prev_env = std::env::var_os("DESLICER_NO_INPUT");
        std::env::remove_var("CI");
        std::env::remove_var("DESLICER_NO_INPUT");
        set_no_input(true);
        assert!(is_non_interactive());
        set_no_input(false);
        match prev_ci {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        match prev_env {
            Some(v) => std::env::set_var("DESLICER_NO_INPUT", v),
            None => std::env::remove_var("DESLICER_NO_INPUT"),
        }
    }
}
