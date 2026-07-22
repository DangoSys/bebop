use std::path::PathBuf;

#[derive(Debug)]
pub struct Cli {
    pub log_dir: Option<PathBuf>,
    pub harts: Option<u32>,
}

pub fn parse() -> Result<Cli, String> {
    let mut args = std::env::args().skip(1);
    let mut log_dir = None;
    let mut harts = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--log-dir" => log_dir = Some(PathBuf::from(take(&mut args, "--log-dir")?)),
            "--harts" => {
                harts = Some(
                    take(&mut args, "--harts")?
                        .parse()
                        .map_err(|e| format!("invalid --harts value: {e}"))?,
                )
            }
            "--help" | "-h" => {
                help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument `{arg}`")),
        }
    }

    Ok(Cli { log_dir, harts })
}

fn take(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{name} requires a value"))
}

fn help() {
    println!("Usage: bebop-termial [--log-dir <dir>] [--harts <n>]");
}
