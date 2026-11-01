use crate::{sims, BuildCommand, BuildTarget, Cli, Commands, P2eArgs, RunCommand, RunTarget, VcsArgs, VerilatorArgs};
use clap::Parser;
use std::io::Write;

pub fn run_cli() {
    execute(Cli::parse());
}

/// The simulation entry that links this runner: difftest runs its BEMU model.
pub static SIMULATION: std::sync::OnceLock<&'static bebop_bemu::Simulation> = std::sync::OnceLock::new();

pub fn run_backend(backend: &str, simulation: &'static bebop_bemu::Simulation) {
    SIMULATION
        .set(simulation)
        .unwrap_or_else(|_| panic!("runner simulation set twice"));
    let target = match backend {
        "vcs" => RunTarget::Vcs(VcsArgs::parse()),
        "verilator" => RunTarget::Verilator(VerilatorArgs::parse()),
        "p2e" => RunTarget::P2e(P2eArgs::parse()),
        _ => panic!("unknown runner backend: {backend}"),
    };
    execute(Cli {
        command: Commands::Run(RunCommand { target }),
    });
}

fn execute(cli: Cli) {
    let p2e_command = matches!(
        &cli.command,
        Commands::Build(BuildCommand {
            target: BuildTarget::P2e { .. },
        }) | Commands::Run(RunCommand {
            target: RunTarget::P2e(..),
        })
    );
    let result = match cli.command {
        Commands::Config { design } => sims::configure(&design),
        Commands::Build(command) => sims::build(command),
        Commands::Run(command) => sims::run(command),
        Commands::Dasm => bebop_dasm::process_dasm(std::io::stdin().lock(), std::io::stdout().lock())
            .map_err(|error| <snafu::Whatever as snafu::FromString>::without_source(error.to_string())),
    };

    let exit_code = match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    };

    if p2e_command {
        std::io::stdout().flush().expect("failed to flush stdout");
        std::io::stderr().flush().expect("failed to flush stderr");
        unsafe { libc::_exit(exit_code) };
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
