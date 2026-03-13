use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bebop", about = "A buckyball emulator written in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print hello world
    Batch,
    /// Open the GUI window
    #[cfg(feature = "gui")]
    Gui,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Batch) => {
            println!("Hello, world!");
        }
        #[cfg(feature = "gui")]
        Some(Commands::Gui) => {
            bebop_tauri_lib::run();
        }
        None => {
            println!("No command given. Use -h for help.");
        }
    }
}
