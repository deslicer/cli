//! `deslicer change list` — find plan ids from earlier agent steps (#224).

use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::ChangePlan;
use crate::pagination::{print_page_json, PageArgs};
use crate::Ctx;

#[derive(ClapArgs)]
#[command(after_long_help = super::CHANGE_WORKFLOW_EXAMPLES)]
pub struct Args {
    /// Filter by plan status (server-side when Observer supports it).
    #[arg(long)]
    pub status: Option<String>,

    /// Only plans created/updated at or after this RFC3339 timestamp.
    #[arg(long)]
    pub since: Option<String>,

    /// Environment name or tenant slug used to scope listing and CI auth.
    #[arg(long)]
    pub environment: Option<String>,

    #[command(flatten)]
    pub page: PageArgs,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let page = match client
        .list_plans_page(
            args.environment.as_deref(),
            args.status.as_deref(),
            args.since.as_deref(),
            &args.page,
        )
        .await
    {
        Ok(page) => page,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    match ctx.log_format {
        LogFormat::Json => print_page_json(&page),
        LogFormat::Human => {
            print!("{}", format_human(&page.items, page.next_cursor.as_deref()));
            0
        }
    }
}

fn format_human(plans: &[ChangePlan], next_cursor: Option<&str>) -> String {
    if plans.is_empty() {
        return "No change plans found.\n".to_string();
    }
    let mut lines = vec!["PLAN ID  STATUS  NAME".to_string()];
    for plan in plans {
        let name = plan
            .name
            .as_deref()
            .map(scrub)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "-".into());
        lines.push(format!(
            "{}  {}  {}",
            scrub(plan.external_id()),
            scrub(&plan.status),
            name
        ));
    }
    if let Some(cursor) = next_cursor {
        lines.push(format!("next_cursor: {cursor}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn scrub(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagination::page_slice;

    #[test]
    fn human_lists_plan_id_and_status() {
        let plans = vec![ChangePlan {
            id: "row".into(),
            plan_id: Some("ext-1".into()),
            status: "pending_approval".into(),
            name: Some("demo".into()),
            summary: None,
        }];
        let text = format_human(&plans, Some("50"));
        assert!(text.contains("ext-1"));
        assert!(text.contains("pending_approval"));
        assert!(text.contains("next_cursor: 50"));
    }

    #[test]
    fn page_slice_caps_limit() {
        let plans: Vec<ChangePlan> = (0..80)
            .map(|i| ChangePlan {
                id: format!("r{i}"),
                plan_id: Some(format!("p{i}")),
                status: "draft".into(),
                name: None,
                summary: None,
            })
            .collect();
        let page = page_slice(
            &plans,
            &PageArgs {
                limit: 50,
                cursor: None,
            },
        );
        assert_eq!(page.items.len(), 50);
        assert_eq!(page.next_cursor.as_deref(), Some("50"));
    }
}
