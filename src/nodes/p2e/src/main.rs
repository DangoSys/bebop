use bebop_p2e::{run, P2ECli};
use std::path::PathBuf;

fn main() {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut buildbitstream = false;
    let mut runworkload = false;
    let mut config: Option<String> = None;
    let mut image: Option<PathBuf> = None;
    let mut bitstream: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--buildbitstream" => buildbitstream = true,
            "--runworkload" => runworkload = true,
            "--config" => {
                if i + 1 < args.len() {
                    config = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--image" => {
                if i + 1 < args.len() {
                    image = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--bitstream" => {
                if i + 1 < args.len() {
                    bitstream = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let cli = P2ECli {
        buildbitstream,
        runworkload,
        config,
        image,
        bitstream,
    };

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
