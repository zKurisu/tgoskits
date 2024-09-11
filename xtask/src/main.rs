#![feature(path_file_prefix)]

mod compile;
mod project;
mod qemu;
mod shell;

use anyhow::Result;
use byte_unit::Byte;
use clap::{Args, Parser, Subcommand, ValueEnum};
use project::Project;
use qemu::Qemu;
use std::{fs::File, io::Read};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Build(BuildArgs),
    Qemu(QemuArgs),
}

#[derive(Args, Debug)]
struct BuildArgs {
    #[arg(short, long)]
    config: Option<String>,
}
#[derive(Args, Debug, Default)]
struct QemuArgs {
    #[arg(short, long)]
    config: Option<String>,
    #[arg(short, long)]
    debug: bool,
    #[arg(long)]
    dtb: bool,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    match &args.command {
        Commands::Build(a) => {
            let mut project = Project::new(a.config.as_deref(), false)?;
            project.build();
        }
        Commands::Qemu(a) => {
            let mut project = Project::new(a.config.as_deref(), a.debug)?;
            let qemu = project.qemu(a.dtb);
        }
    }

    Ok(())
}
