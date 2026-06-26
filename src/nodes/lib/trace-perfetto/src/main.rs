use bebop_trace_perfetto::{convert_ndjson_writer, ConvertOptions};
use clap::Parser;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "bebop-trace-perfetto")]
#[command(about = "Convert Buckyball NDJSON trace to Perfetto Trace Event JSON")]
struct Cli {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 1)]
    tick_ns: u64,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    if cli.tick_ns == 0 {
        return Err("tick_ns must be > 0".into());
    }

    let input_file = File::open(&cli.input)?;
    let output_file = File::create(&cli.output)?;
    let reader = BufReader::new(input_file);
    let writer = BufWriter::new(output_file);
    let options = ConvertOptions { tick_ns: cli.tick_ns };
    convert_ndjson_writer(reader, writer, &options)?;
    Ok(())
}
