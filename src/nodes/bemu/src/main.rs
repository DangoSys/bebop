use bebop_bemu::{run, BemuCli};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <elf> [args...]", args[0]);
        std::process::exit(1);
    }

    let elf = PathBuf::from(&args[1]);
    let sim_args = args[2..].to_vec();

    let cli = BemuCli {
        elf,
        args: sim_args,
    };

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
