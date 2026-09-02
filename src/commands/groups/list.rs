use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::HostGroup;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let groups = match client.list_groups().await {
        Ok(groups) => groups,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&groups) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize groups: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_groups_human(&groups));
            0
        }
    }
}

fn format_groups_human(groups: &[HostGroup]) -> String {
    if groups.is_empty() {
        return "No host groups. Create one in the portal, then rerun this command.\n".to_string();
    }
    let mut lines = vec!["ID  NAME  MEMBERS".to_string()];
    for group in groups {
        let name = group.display_name.as_deref().unwrap_or(&group.name);
        let members = group
            .member_count
            .map(|count| count.to_string())
            .unwrap_or_else(|| "-".to_string());
        lines.push(format!("{}  {}  {}", group.id, name, members));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_list_uses_display_name_and_id() {
        let text = format_groups_human(&[HostGroup {
            id: "019f36d6-3f61-7eea-9417-7ac4a8a10f69".into(),
            name: "search-heads".into(),
            display_name: Some("Search Heads".into()),
            member_count: Some(2),
        }]);
        assert!(text.contains("019f36d6-3f61-7eea-9417-7ac4a8a10f69"));
        assert!(text.contains("Search Heads"));
        assert!(text.contains('2'));
        assert!(!text.contains('{'));
    }

    #[test]
    fn human_list_explains_empty() {
        let text = format_groups_human(&[]);
        assert!(text.contains("No host groups"));
    }
}
