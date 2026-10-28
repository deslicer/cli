use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::org::client::OrgClient;
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Tenant id or slug to switch to
    pub tenant: String,

    /// Perform the switch. Without this flag, print a dry-run and exit.
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
                    "action": "org_switch",
                    "tenant": args.tenant,
                })
            ),
            LogFormat::Human => println!(
                "dry-run: would switch active org/tenant to {}\npass --yes to switch",
                args.tenant
            ),
        }
        return 0;
    }

    let client = match OrgClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.switch_tenant(&args.tenant).await {
        Ok(session) => match ctx.log_format {
            LogFormat::Json => {
                let payload = json!({
                    "ok": true,
                    "tenant_id": session.tenant_id,
                    "tenant_slug": session.tenant_slug,
                    "display_name": session.display_name,
                    "observer_api_url": session.observer_api_url,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into())
                );
                0
            }
            LogFormat::Human => {
                println!(
                    "switched to tenant {} ({})",
                    session.tenant_slug.as_deref().unwrap_or(&session.tenant_id),
                    session.display_name
                );
                0
            }
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_requires_yes() {
        let mut cmd = crate::cli::Cli::command();
        let org = cmd.find_subcommand_mut("org").expect("org");
        let switch = org.find_subcommand_mut("switch").expect("switch");
        let mut buf = Vec::new();
        switch.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
    }
}
