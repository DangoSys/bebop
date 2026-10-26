use bebop_report::{junit, RegressionResult};

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    /// JSON array of RegressionResult records from one regression invocation.
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    junit: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let records: Vec<RegressionResult> = serde_json::from_slice(&std::fs::read(&args.input)?)?;
    if records.is_empty() {
        return Err("No regression results to export".into());
    }
    let results: Vec<_> = records
        .into_iter()
        .map(|record| (record.workload_name.clone(), record))
        .collect();
    if let Some(parent) = args.junit.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.junit)?;
    junit::write(&mut output, "bebop-regression", &results, 0)?;
    Ok(())
}
