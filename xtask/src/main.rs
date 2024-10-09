#![feature(path_file_prefix)]
#![feature(option_get_or_insert_default)]

mod compile;
mod project;
mod qemu;
mod shell;
mod uboot;

use anyhow::Result;
use clap::*;
use project::Project;
use qemu::Qemu;
use uboot::UBoot;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    config: Option<String>,
    #[command(subcommand)]
    command: SubCommands,
}

#[derive(Subcommand, Debug)]
enum SubCommands {
    Build,
    Qemu(QemuArgs),
    Uboot,
}

#[derive(Args, Debug)]
struct BuildArgs {}
#[derive(Args, Debug, Default)]
struct QemuArgs {
    #[arg(short, long)]
    debug: bool,
    #[arg(long)]
    dtb: bool,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let mut project = Project::new(args.config)?;

    match args.command {
        SubCommands::Build => {
            project.build(false)?;
        }
        SubCommands::Qemu(a) => {
            project.build(a.debug)?;
            Qemu::run(&project, a)?;
        }
        SubCommands::Uboot => {
            project.build(false)?;
            UBoot::run(&project)?;
        }
    }

    Ok(())
}
