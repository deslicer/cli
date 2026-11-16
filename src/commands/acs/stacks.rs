use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::AcsStack;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct ListArgs {
    /// Deslicer environment profile used for auth
    #[arg(long)]
    pub environment: Option<String>,

    /// Case-insensitive substring matched against the stack name.
    #[arg(long)]
    pub filter: Option<String>,
}

pub async fn list(ctx: Ctx, args: ListArgs) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let stacks = match client.list_acs_stacks().await {
        Ok(stacks) => stacks,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let stacks = crate::list_filter::apply(args.filter.as_deref(), stacks, |stack| {
        format!(
            "{} {}",
            stack.name,
            stack.environment.as_deref().unwrap_or("")
        )
    });

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
        const SET: &str = "deslicer cloud stack credential set"; // pragma: allowlist secret
        return format!(
            "No ACS stacks. Store a stack credential with `{SET} <stack> --token-stdin`.\n"
        );
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
        assert!(format_stacks_human(&[]).contains("cloud stack credential set"));
    }
}
