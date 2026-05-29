use clap::Args as ClapArgs;

use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub plan_id: String,

    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let _ = (ctx, args);
    eprintln!("not implemented");
    1
}
