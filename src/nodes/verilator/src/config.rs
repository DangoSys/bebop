use clap::Parser;
use snafu::{FromString, Whatever};

#[derive(Debug, Parser)]
#[command(name = "bebop-verilator", about = "Bebop Verilator Simulator")]
struct CliArgs {
    #[arg(long, help = "Specify log directory (will create bdb.ndjson and stdout.log)")]
    log_dir: String,
    #[arg(long, help = "Specify waveform directory (will create waveform.fst)")]
    fst_dir: String,
    #[arg(long, help = "Enable instruction trace")]
    itrace: bool,
    #[arg(long, help = "Enable memory trace")]
    mtrace: bool,
    #[arg(long, help = "Enable performance counter trace")]
    pmctrace: bool,
    #[arg(long, help = "Enable cycle counter trace")]
    ctrace: bool,
    #[arg(long, help = "Enable bank trace")]
    banktrace: bool,
}

pub fn parse_args(args: Vec<String>) -> Result<(String, String, bool, bool, bool, bool, bool), Whatever> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push("bebop-verilator".to_string());
    argv.extend(args);
    let parsed = CliArgs::try_parse_from(argv).map_err(|e| Whatever::without_source(e.to_string()))?;
    Ok((
        parsed.log_dir,
        parsed.fst_dir,
        parsed.itrace,
        parsed.mtrace,
        parsed.pmctrace,
        parsed.ctrace,
        parsed.banktrace,
    ))
}
