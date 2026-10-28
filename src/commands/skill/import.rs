use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::commands::skill::client::SkillClient;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Marketplace id, URL, or local path recognized by the portal
    pub source: String,

    /// Optional rename on import
    #[arg(long)]
    pub name: Option<String>,

    /// Perform the import. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if !args.yes {
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "import",
                    "source": args.source,
                    "name": args.name,
                })
            ),
            LogFormat::Human => {
                let rename = args
                    .name
                    .as_deref()
                    .map(|n| format!(" as {n}"))
                    .unwrap_or_default();
                println!(
                    "dry-run: would import skill from {}{rename}\npass --yes to import (scan results shown after)",
                    args.source
                );
            }
        }
        return 0;
    }

    let client = match SkillClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.import(&args.source, args.name.as_deref()).await {
        Ok(result) => match ctx.log_format {
            LogFormat::Json => match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{json}");
                    0
                }
                Err(err) => {
                    eprintln!("failed to serialize import result: {err}");
                    1
                }
            },
            LogFormat::Human => {
                print!("{}", format_import_human(&result));
                0
            }
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn format_import_human(result: &crate::commands::skill::client::SkillImportResult) -> String {
    let mut lines = Vec::new();
    if let Some(msg) = result.message.as_deref() {
        lines.push(msg.to_string());
    }
    if let Some(skill) = result.skill.as_ref() {
        lines.push(format!(
            "imported skill {} ({})",
            skill.name.as_deref().unwrap_or(&skill.id),
            skill.id
        ));
    } else {
        lines.push("import completed".to_string());
    }
    if let Some(renamed) = result.renamed_to.as_deref() {
        lines.push(format!("renamed to: {renamed}"));
    }
    if let Some(size) = result
        .size_bytes
        .or(result.skill.as_ref().and_then(|s| s.size_bytes))
    {
        lines.push(format!("size: {size} bytes"));
    }
    let scan = result
        .scan_status
        .as_deref()
        .or(result.skill.as_ref().and_then(|s| s.scan_status.as_deref()));
    if let Some(status) = scan {
        let findings = result
            .scan_findings
            .or(result.skill.as_ref().and_then(|s| s.scan_findings))
            .unwrap_or(0);
        lines.push(format!("scan: {status} ({findings} findings)"));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_requires_yes() {
        let mut cmd = crate::cli::Cli::command();
        let skill = cmd.find_subcommand_mut("skill").expect("skill");
        let import = skill.find_subcommand_mut("import").expect("import");
        let mut buf = Vec::new();
        import.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
    }
}
