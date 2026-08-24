use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::Ctx;

use super::jti::parse_jti;

#[derive(ClapArgs)]
pub struct Args {
    /// Token id from `deslicer enroll list` (UUID only)
    #[arg(long)]
    pub jti: String,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    let jti = parse_jti(&args.jti)?;
    let (session, client) = authenticate(&ctx, None, None).await?;
    if !session.is_device_session() {
        return Err(CliError::Other(
            "`enroll revoke` requires `deslicer auth login` (device session)".into(),
        ));
    }

    client.revoke_enrollment_token(&jti).await?;
    match ctx.log_format {
        LogFormat::Json => println!("{}", serde_json::json!({ "revoked": jti })),
        LogFormat::Human => println!("Revoked enrollment token {jti}."),
    }
    Ok(0)
}
