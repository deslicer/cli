use std::io;

use clap::{CommandFactory, ValueEnum};
use clap_complete::{generate, Shell};

use crate::cli::Cli;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl CompletionShell {
    fn clap_shell(self) -> Shell {
        match self {
            Self::Bash => Shell::Bash,
            Self::Zsh => Shell::Zsh,
            Self::Fish => Shell::Fish,
        }
    }
}

#[derive(clap::Args)]
pub struct Args {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: CompletionShell,
}

pub fn run(args: Args) -> i32 {
    let mut cmd = Cli::command();
    generate(
        args.shell.clap_shell(),
        &mut cmd,
        "deslicer",
        &mut io::stdout(),
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct CompletionProbe {
        #[command(flatten)]
        args: Args,
    }

    #[test]
    fn bash_completions_name_the_deslicer_binary() {
        let probe = CompletionProbe::try_parse_from(["probe", "bash"]).expect("parse");
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        generate(
            probe.args.shell.clap_shell(),
            &mut cmd,
            "deslicer",
            &mut buf,
        );
        let script = String::from_utf8(buf).expect("utf8");
        assert!(script.contains("deslicer"));
        assert!(script.contains("auth") || script.contains("complete"));
    }
}
