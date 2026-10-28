use clap::{Args as ClapArgs, Subcommand};

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::AcsStack;
use crate::Ctx;

#[derive(Subcommand)]
pub enum StacksCmd {
    /// List ACS stacks
    List(ListArgs),
}

#[derive(ClapArgs)]
pub struct ListArgs {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn dispatch(ctx: Ctx, cmd: StacksCmd) -> i32 {
    match cmd {
        StacksCmd::List(args) => list(ctx, args).await,
    }
}

async fn list(ctx: Ctx, args: ListArgs) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let stacks = match client.list_acs_stacks().await {
        Ok(stacks) => stacks,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&stacks) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize ACS stacks: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_stacks_human(&stacks));
            0
        }
    }
}

fn format_stacks_human(stacks: &[AcsStack]) -> String {
    if stacks.is_empty() {
        return "No ACS stacks.\n".to_string();
    }
    let mut lines = vec!["NAME  STATUS  REGION".to_string()];
    for stack in stacks {
        lines.push(format!(
            "{}  {}  {}",
            stack.name,
            stack.status.as_deref().unwrap_or("-"),
            stack.region.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_list_rows() {
        let text = format_stacks_human(&[AcsStack {
            name: "prod".into(),
            status: Some("ready".into()),
            region: Some("us-east-1".into()),
        }]);
        assert!(text.contains("prod"));
        assert!(text.contains("ready"));
    }
}
