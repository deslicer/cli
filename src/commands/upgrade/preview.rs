use super::{output_format, print_json, rolling_request, RollingArgs};
use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::UpgradePreview;
use crate::Ctx;

pub async fn run(ctx: Ctx, args: RollingArgs) -> i32 {
    let format = output_format(&ctx, args.json);
    match run_inner(&ctx, &args, format).await {
        Ok(code) => code,
        Err(err) => map_cli_error(format, err),
    }
}

async fn run_inner(ctx: &Ctx, args: &RollingArgs, format: LogFormat) -> Result<i32, CliError> {
    super::validate_version(&args.to_version)?;
    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let body = rolling_request(&client, args).await?;
    let preview = client.upgrade_preview(&body).await?;
    Ok(match format {
        LogFormat::Json => print_json(&preview),
        LogFormat::Human => {
            print!("{}", format_human(&preview));
            0
        }
    })
}

pub(crate) fn format_human(p: &UpgradePreview) -> String {
    let mut lines = vec![format!(
        "{} → {} (catalog {}), {} wave(s); next publishable wave: {}",
        p.family,
        p.to_version,
        if p.catalog_revision.is_empty() {
            "-"
        } else {
            &p.catalog_revision
        },
        p.waves.len(),
        p.publishable_wave_index
            .map(|i| i.to_string())
            .unwrap_or_else(|| "none".into())
    )];
    lines.push("WAVE  STEP  HOSTS  POLICY  OUTCOME".into());
    for w in &p.waves {
        lines.push(format!(
            "{}  {}  {}  {}  {}",
            w.wave_index,
            w.step_id,
            w.host_ids.len(),
            w.serial_policy,
            w.outcome
        ));
    }
    lines.push("read-only preview: no plan was created".into());
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_lists_waves() {
        let p: UpgradePreview = serde_json::from_value(serde_json::json!({
            "family": "upgrade_rolling_clusters", "catalog_revision": "r1",
            "applied_overlay_ids": [], "to_version": "9.4.1", "publishable_wave_index": 0,
            "waves": [{"wave_index": 0, "step_id": "precheck", "host_scope": "all",
                       "serial_policy": "parallel", "host_ids": ["a", "b"],
                       "sub_batch_count": 1, "publishable": true, "outcome": "pending"}]
        }))
        .unwrap();
        let text = format_human(&p);
        assert!(text.contains("next publishable wave: 0"));
        assert!(text.contains("0  precheck  2  parallel  pending"));
    }
}
