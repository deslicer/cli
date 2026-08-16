use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::InventoryGroup;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    let groups = match client.list_inventory().await {
        Ok(groups) => groups,
        Err(err) => return map_cli_error(err),
    };

    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&groups) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize inventory: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_inventory_human(&groups));
            0
        }
    }
}

fn format_inventory_human(groups: &[InventoryGroup]) -> String {
    if groups.is_empty() {
        return "No inventory groups. Assign hosts in the portal, then rerun this command.\n"
            .to_string();
    }
    let mut lines = vec!["GROUP  MEMBERS  HOSTS".to_string()];
    for group in groups {
        lines.push(format!(
            "{}  {}  {}",
            group.name,
            group.hosts.len(),
            host_column(group)
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn host_column(group: &InventoryGroup) -> String {
    if !group.hosts.is_empty() {
        return group.hosts.join(", ");
    }
    if !group.children.is_empty() {
        return format!("(children: {})", group.children.join(", "));
    }
    "-".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_list_shows_hosts_and_children() {
        let text = format_inventory_human(&[
            InventoryGroup {
                name: "all".into(),
                hosts: vec![],
                children: vec!["forwarders".into()],
            },
            InventoryGroup {
                name: "forwarders".into(),
                hosts: vec!["idx1".into()],
                children: vec![],
            },
        ]);
        assert!(text.contains("forwarders  1  idx1"));
        assert!(text.contains("all  0  (children: forwarders)"));
        assert!(!text.contains('{'));
    }

    #[test]
    fn human_list_explains_empty() {
        let text = format_inventory_human(&[]);
        assert!(text.contains("No inventory groups"));
    }
}
