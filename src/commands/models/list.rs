use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::models::client::{ModelSummary, ModelsClient};
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    let client = match ModelsClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let models = match client.list().await {
        Ok(models) => models,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&models) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize models: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_models_human(&models));
            0
        }
    }
}

fn format_models_human(models: &[ModelSummary]) -> String {
    if models.is_empty() {
        return "No models available for this tenant.\n".to_string();
    }
    let mut lines = vec!["ID  NAME  PROVIDER  BYOK  STATUS".to_string()];
    for m in models {
        lines.push(format!(
            "{}  {}  {}  {}  {}",
            m.id,
            m.name.as_deref().unwrap_or("-"),
            m.provider.as_deref().unwrap_or("-"),
            m.byok.map(|b| if b { "yes" } else { "no" }).unwrap_or("-"),
            m.status.as_deref().unwrap_or("-")
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
        let text = format_models_human(&[ModelSummary {
            id: "gpt".into(),
            name: Some("GPT".into()),
            provider: Some("openai".into()),
            byok: Some(true),
            status: Some("ready".into()),
        }]);
        assert!(text.contains("openai"));
        assert!(text.contains("yes"));
    }
}
