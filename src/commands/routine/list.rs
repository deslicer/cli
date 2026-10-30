use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::commands::routine::client::{RoutineClient, RoutineSummary};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    let client = match RoutineClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let routines = match client.list().await {
        Ok(routines) => routines,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&routines) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize routines: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_routines_human(&routines));
            0
        }
    }
}

fn format_routines_human(routines: &[RoutineSummary]) -> String {
    if routines.is_empty() {
        return "No routines. Create one in the portal first.\n".to_string();
    }
    let mut lines = vec!["ID  NAME  ENABLED  MODEL  SCHEDULE  STATUS".to_string()];
    for r in routines {
        lines.push(format!(
            "{}  {}  {}  {}  {}  {}",
            r.id,
            r.name.as_deref().unwrap_or("-"),
            r.enabled
                .map(|e| if e { "yes" } else { "no" })
                .unwrap_or("-"),
            r.model.as_deref().unwrap_or("-"),
            r.schedule.as_deref().unwrap_or("-"),
            r.status.as_deref().unwrap_or("-")
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
        let text = format_routines_human(&[RoutineSummary {
            id: "r1".into(),
            name: Some("nightly".into()),
            enabled: Some(true),
            schedule: Some("0 2 * * *".into()),
            model: Some("default".into()),
            status: Some("ok".into()),
        }]);
        assert!(text.contains("nightly"));
        assert!(text.contains("yes"));
    }
}
