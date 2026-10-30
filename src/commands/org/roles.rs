use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::org::client::{OrgClient, OrgRole};
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    let client = match OrgClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let roles = match client.list_roles().await {
        Ok(roles) => roles,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&roles) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize roles: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_roles_human(&roles));
            0
        }
    }
}

fn format_roles_human(roles: &[OrgRole]) -> String {
    if roles.is_empty() {
        return "No roles.\n".to_string();
    }
    let mut lines = vec!["ID  NAME  DESCRIPTION".to_string()];
    for r in roles {
        lines.push(format!(
            "{}  {}  {}",
            r.id,
            r.name.as_deref().unwrap_or("-"),
            r.description.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}
