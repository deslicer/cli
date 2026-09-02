use super::*;
use tempfile::tempdir;

fn known(names: &[&str]) -> HashSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

fn write_app(root: &Path, rel: &str) {
    let default = root.join(rel).join("default");
    std::fs::create_dir_all(&default).unwrap();
    std::fs::write(default.join("app.conf"), "").unwrap();
}

#[test]
fn accepts_valid_multi_app_file() {
    let dir = tempdir().unwrap();
    write_app(dir.path(), "apps/ta_nix");
    let yaml = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n\
\x20\x20\x20\x20\x20\x20\x20\x20dest_dir: apps\n";
    let report =
        validate_environment_yaml(yaml, "prod.yml", dir.path(), Some(&known(&["indexers"])));
    assert!(report.is_ok(), "{:?}", report.issues);
}

#[test]
fn rejects_missing_destinations() {
    let report = validate_environment_yaml("tenant_id: x\n", "x.yml", Path::new("."), None);
    assert!(!report.is_ok());
    assert!(report.issues[0].message.contains("destinations"));
}

#[test]
fn rejects_duplicate_inventory_groups() {
    let yaml = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps: []\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps: []\n";
    let report = validate_environment_yaml(yaml, "prod.yml", Path::new("."), None);
    assert!(report
        .errors()
        .any(|issue| issue.message.contains("duplicate inventory_group")));
}

#[test]
fn rejects_duplicate_source_path_dest_dir() {
    let dir = tempdir().unwrap();
    write_app(dir.path(), "apps/ta_nix");
    let yaml = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n\
\x20\x20\x20\x20\x20\x20\x20\x20dest_dir: apps\n";
    let report =
        validate_environment_yaml(yaml, "prod.yml", dir.path(), Some(&known(&["indexers"])));
    assert!(report
        .errors()
        .any(|issue| issue.message.contains("duplicate app")));
}

#[test]
fn rejects_missing_source_path_unless_absent() {
    let yaml = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/missing\n";
    let report = validate_environment_yaml(
        yaml,
        "prod.yml",
        Path::new("/tmp"),
        Some(&known(&["indexers"])),
    );
    assert!(report
        .errors()
        .any(|issue| issue.message.contains("does not exist")));

    let absent = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/missing\n\
\x20\x20\x20\x20\x20\x20\x20\x20state: absent\n";
    let report = validate_environment_yaml(
        absent,
        "prod.yml",
        Path::new("/tmp"),
        Some(&known(&["indexers"])),
    );
    assert!(
        report
            .errors()
            .all(|issue| !issue.message.contains("does not exist")),
        "{:?}",
        report.issues
    );
}

#[test]
fn rejects_invalid_dest_dir() {
    let dir = tempdir().unwrap();
    write_app(dir.path(), "apps/ta_nix");
    let yaml = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n\
\x20\x20\x20\x20\x20\x20\x20\x20dest_dir: not-a-real-dir\n";
    let report =
        validate_environment_yaml(yaml, "prod.yml", dir.path(), Some(&known(&["indexers"])));
    assert!(report
        .errors()
        .any(|issue| issue.message.contains("invalid dest_dir")));
}

#[test]
fn rejects_unknown_live_group() {
    let dir = tempdir().unwrap();
    write_app(dir.path(), "apps/ta_nix");
    let yaml = "destinations:\n\
\x20\x20- inventory_group: ghost\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n";
    let report =
        validate_environment_yaml(yaml, "prod.yml", dir.path(), Some(&known(&["indexers"])));
    assert!(report
        .errors()
        .any(|issue| issue.message.contains("unknown inventory_group \"ghost\"")));
}

#[test]
fn resolve_env_file_prefers_yml() {
    let dir = tempdir().unwrap();
    let env = dir.path().join(".deslicer/environments");
    std::fs::create_dir_all(&env).unwrap();
    std::fs::write(env.join("prod.yml"), "destinations: []\n").unwrap();
    let path = resolve_env_file(dir.path(), "prod").unwrap();
    assert!(path.ends_with("prod.yml"));
}
