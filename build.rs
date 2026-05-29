fn main() {
    let sha = std::env::var("DESLICER_GIT_SHA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(git_short_sha)
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=DESLICER_GIT_SHA={sha}");
    println!("cargo:rerun-if-env-changed=DESLICER_GIT_SHA");
    println!("cargo:rerun-if-changed=.git/HEAD");
}

fn git_short_sha() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}
