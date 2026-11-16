//! `docs/commands.md` stays aligned with the clap tree (#185).

use deslicer_cli::schema::build_schema; // pragma: allowlist secret

#[test]
fn command_reference_lists_every_schema_path() {
    let doc = include_str!("../docs/commands.md");
    let schema = build_schema(None).expect("schema");
    let commands = schema["commands"].as_array().expect("commands");
    assert!(commands.len() > 50, "schema unexpectedly small");
    for cmd in commands {
        let path = cmd["path"].as_str().expect("path");
        let needle = format!("`{path}`");
        assert!(
            doc.contains(&needle),
            "docs/commands.md is missing {needle}; regenerate it from `schema`"
        );
    }
    for phrase in [
        "cloud stack credential",
        "routine list",
        "remediate analyse",
        "model byok",
        "billing usage",
        "inventory apps",
        "environment mapping",
        "agent clone",
        "--from-registry",
        "repo bootstrap --check",
        "org settings",
        "skill import",
        "license bind",
        "catalog plan",
        "upgrade readiness",
        "repo doctor",
    ] {
        assert!(doc.contains(phrase), "missing {phrase}");
    }
}
