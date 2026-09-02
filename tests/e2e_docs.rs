use clap::CommandFactory;
use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::{Cli, LogFormat};
use deslicer_cli::commands::docs::{self, Args};
use deslicer_cli::Ctx;
use url::Url;

fn ctx(format: LogFormat) -> Ctx {
    Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: None,
        ci_override: Some(CiPlatform::Local),
        log_format: format,
    }
}

#[test]
fn docs_list_and_topic_print() {
    let code = docs::run(
        ctx(LogFormat::Human),
        Args {
            topic: None,
            print: false,
            open: false,
        },
    );
    assert_eq!(code, 0);

    let code = docs::run(
        ctx(LogFormat::Human),
        Args {
            topic: Some("path-a2".into()),
            print: true,
            open: false,
        },
    );
    assert_eq!(code, 0);
}

#[test]
fn docs_unknown_topic_is_nonzero() {
    let code = docs::run(
        ctx(LogFormat::Human),
        Args {
            topic: Some("nope".into()),
            print: true,
            open: false,
        },
    );
    assert_eq!(code, 1);
}

#[test]
fn docs_json_lists_topics() {
    let code = docs::run(
        ctx(LogFormat::Json),
        Args {
            topic: Some("init".into()),
            print: false,
            open: false,
        },
    );
    assert_eq!(code, 0);
}

#[test]
fn docs_api_keys_is_a_known_topic() {
    let code = docs::run(
        ctx(LogFormat::Human),
        Args {
            topic: Some("api-keys".into()),
            print: true,
            open: false,
        },
    );
    assert_eq!(code, 0);
}

#[test]
fn cli_help_includes_docs_and_init_path_a2() {
    let mut cmd = Cli::command();
    let mut buf = Vec::new();
    cmd.write_long_help(&mut buf).expect("help");
    let root = String::from_utf8(buf).expect("utf8");
    assert!(root.contains("docs"));

    let mut init_buf = Vec::new();
    cmd.find_subcommand_mut("init")
        .expect("init")
        .write_long_help(&mut init_buf)
        .expect("init help");
    let init_help = String::from_utf8(init_buf).expect("utf8");
    assert!(init_help.contains("github-token"));
    assert!(init_help.contains("deslicer docs path-a2"));
}
