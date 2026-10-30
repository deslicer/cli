use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub host: Option<String>,

    #[arg(long)]
    pub app: Option<String>,

    #[arg(long)]
    pub group: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let analysis = match client
        .remediate_analyse(
            args.host.as_deref(),
            args.app.as_deref(),
            args.group.as_deref(),
        )
        .await
    {
        Ok(analysis) => analysis,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&analysis) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize analysis: {err}");
                1
            }
        },
        LogFormat::Human => {
            if let Some(status) = analysis.status.as_deref() {
                println!("status: {status}");
            }
            if let Some(summary) = analysis.summary.as_deref() {
                println!("summary: {summary}");
            }
            if let Some(host) = analysis.host.as_deref() {
                println!("host: {host}");
            }
            if let Some(app) = analysis.app.as_deref() {
                println!("app: {app}");
            }
            if let Some(findings) = analysis.findings.as_ref() {
                match crate::json_doc::to_pretty_string(findings) {
                    Ok(json) => println!("{json}"),
                    Err(err) => {
                        eprintln!("failed to format findings: {err}");
                        return 1;
                    }
                }
            }
            eprintln!("Create a draft plan with: deslicer remediate draft --yes …");
            0
        }
    }
}
