use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::commands;
use crate::doctor;

#[derive(Parser, Debug)]
#[command(
    name = "proto-importer",
    version,
    about = "Python proto importer toolkit"
)]
pub struct Cli {
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Doctor,
    Build {
        #[arg(long)]
        pyproject: Option<String>,
        #[arg(long)]
        no_verify: bool,
        #[arg(long)]
        postprocess_only: bool,
    },
    Check {
        #[arg(long)]
        pyproject: Option<String>,
    },
    Clean {
        #[arg(long)]
        pyproject: Option<String>,
        #[arg(long)]
        yes: bool,
    },
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| level.to_string());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(env_filter))
        .with_target(false)
        .without_time()
        .init();
}

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    match cli.command {
        Commands::Doctor => doctor::run()?,
        Commands::Build {
            pyproject,
            no_verify,
            postprocess_only,
        } => commands::build(pyproject.as_deref(), no_verify, postprocess_only)?,
        Commands::Check { pyproject } => commands::check(pyproject.as_deref())?,
        Commands::Clean { pyproject, yes } => commands::clean(pyproject.as_deref(), yes)?,
    }
    Ok(())
}

pub fn run_cli_with<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut v: Vec<String> = args.into_iter().map(Into::into).collect();
    if v.is_empty() || v.first().map(|s| s.is_empty()).unwrap_or(true) {
        v.insert(0, "proto-importer".to_string());
    }

    let cli = Cli::parse_from(v);
    init_tracing(cli.verbose);
    match cli.command {
        Commands::Doctor => doctor::run()?,
        Commands::Build {
            pyproject,
            no_verify,
            postprocess_only,
        } => commands::build(pyproject.as_deref(), no_verify, postprocess_only)?,
        Commands::Check { pyproject } => commands::check(pyproject.as_deref())?,
        Commands::Clean { pyproject, yes } => commands::clean(pyproject.as_deref(), yes)?,
    }
    Ok(())
}
