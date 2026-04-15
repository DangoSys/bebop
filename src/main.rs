mod framework;
mod graph;
mod node;

use crate::framework::cli::cli::{dispatch, Cli};
use crate::framework::node::{init_node, is_node0, kill_all_children};
use crate::framework::utils::log::init_log;
use clap::Parser;

fn main() {
    let cli = Cli::parse();

    if let Err(e) = init_node(cli.node_file.as_deref()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    if is_node0() {
        ctrlc::set_handler(|| {
            let _ = kill_all_children();
            std::process::exit(130);
        })
        .map_err(|e| format!("set ctrlc handler: {e}"))
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });
    }

    init_log(cli.verbose);

    let out = dispatch(cli);
    if is_node0() {
        let _ = kill_all_children();
    }
    if let Err(e) = out {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
