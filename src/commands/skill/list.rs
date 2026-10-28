use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::commands::skill::client::{SkillClient, SkillSummary};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    let client = match SkillClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let skills = match client.list().await {
        Ok(skills) => skills,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&skills) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize skills: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_skills_human(&skills));
            0
        }
    }
}

fn format_skills_human(skills: &[SkillSummary]) -> String {
    if skills.is_empty() {
        return "No skills. Import one with `deslicer skill import <source>`.\n".to_string();
    }
    let mut lines =
        vec!["ID  NAME  RENAMED_FROM  SIZE  SCAN  FINDINGS  SOURCE  VERSION".to_string()];
    for s in skills {
        lines.push(format!(
            "{}  {}  {}  {}  {}  {}  {}  {}",
            s.id,
            s.name.as_deref().unwrap_or("-"),
            s.renamed_from.as_deref().unwrap_or("-"),
            s.size_bytes
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into()),
            s.scan_status.as_deref().unwrap_or("-"),
            s.scan_findings
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into()),
            s.source.as_deref().unwrap_or("-"),
            s.version.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_list_shows_size_rename_scan() {
        let text = format_skills_human(&[SkillSummary {
            id: "s1".into(),
            name: Some("ops".into()),
            renamed_from: Some("ops-kit".into()),
            size_bytes: Some(4096),
            scan_status: Some("clean".into()),
            scan_findings: Some(0),
            source: Some("marketplace".into()),
            version: Some("1.0.0".into()),
        }]);
        assert!(text.contains("ops-kit"));
        assert!(text.contains("4096"));
        assert!(text.contains("clean"));
    }
}
