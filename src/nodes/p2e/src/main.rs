use bebop_p2e::{config, run, P2ECli};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    if args[1..].iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{}", config::help_text());
        return;
    }

    let options = match config::parse_args(args[1..].to_vec()) {
        Ok(options) => options,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let cli = P2ECli {
        buildbitstream: options.buildbitstream,
        runworkload: options.runworkload,
    };

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
