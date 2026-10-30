use clap::{Args as ClapArgs, Subcommand};

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::AcsStack;
use crate::Ctx;

#[derive(Subcommand)]
pub enum StacksCmd {
    /// List ACS stacks that have a stored credential
    ///
    /// Examples:
    ///   deslicer acs stacks list
    ///   deslicer --log-format json acs stacks list
    List(ListArgs),
}

#[derive(ClapArgs)]
pub struct ListArgs {
    /// Deslicer environment profile used for auth
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
        LogFormat::Json => match crate::json_doc::to_pretty_string(&stacks) {
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
        return "No ACS stacks. Store a stack credential with `deslicer acs credentials set <stack> --token-stdin`.\n".to_string();
    }
    let mut lines = vec!["NAME  ACS_ENV  TOKEN  UPDATED".to_string()];
    for stack in stacks {
        lines.push(format!(
            "{}  {}  {}  {}",
            stack.name,
            stack.environment.as_deref().unwrap_or("-"),
            if stack.token_configured {
                "set"
            } else {
                "missing"
            },
            stack.updated_at.as_deref().unwrap_or("-")
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
            environment: Some("production".into()),
            token_configured: true,
            updated_at: None,
        }]);
        assert!(text.contains("prod"));
        assert!(text.contains("production"));
        assert!(text.contains("set"));
    }

    #[test]
    fn empty_list_points_at_credentials_set() {
        assert!(format_stacks_human(&[]).contains("acs credentials set"));
    }
}
