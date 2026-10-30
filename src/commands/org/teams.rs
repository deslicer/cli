use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::org::client::{OrgClient, OrgTeam};
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    let client = match OrgClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let teams = match client.list_teams().await {
        Ok(teams) => teams,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&teams) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize teams: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_teams_human(&teams));
            0
        }
    }
}

fn format_teams_human(teams: &[OrgTeam]) -> String {
    if teams.is_empty() {
        return "No teams.\n".to_string();
    }
    let mut lines = vec!["ID  NAME  MEMBERS".to_string()];
    for t in teams {
        lines.push(format!(
            "{}  {}  {}",
            t.id,
            t.name.as_deref().unwrap_or("-"),
            t.member_count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into())
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}
