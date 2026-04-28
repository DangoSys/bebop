use std::path::PathBuf;
use clap::Parser;
use bebop_bemu::{run, BemuCli};

#[derive(Parser, Debug)]
#[command(name = "bebop-bemu")]
#[command(about = "Bebop RISC-V emulator based on Spike")]
struct Cli {
    /// Path to the ELF file to execute
    #[arg(value_name = "ELF")]
    elf: PathBuf,

    /// Additional arguments to pass to the simulator
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    let bemu_cli = BemuCli {
        elf: cli.elf,
        args: cli.args,
    };

    if let Err(e) = run(bemu_cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
