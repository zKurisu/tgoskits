use clap::{Args, Parser, Subcommand, ValueEnum};

mod up_version;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    UpVersion(UpVersionArgs),
}

#[derive(Args, Debug)]
struct UpVersionArgs {
    #[arg(short, long)]
    module: Module,
    #[arg(short, long)]
    break_change: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Module {
    Sparreal,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::UpVersion(args) => up_version::exec(args),
    }
}
