use clap::Parser;
use snafu::Whatever;

#[derive(Debug, Parser)]
#[command(disable_help_flag = true)]
struct CliArgs {
  #[arg(long)]
  log: String,
  #[arg(long)]
  fst: String,
  #[arg(long)]
  trace: Option<String>,
  #[arg(long = "trace-mask")]
  trace_mask: Option<String>,
  #[arg(long)]
  batch: bool,
  #[arg(long)]
  help: bool,
}

pub fn parse_args(
  args: Vec<String>,
) -> Result<(String, String, Option<String>, Option<String>, bool, bool), Whatever> {
  let mut argv = Vec::with_capacity(args.len() + 1);
  argv.push("bebop-verilator".to_string());
  argv.extend(args);
  let parsed = CliArgs::try_parse_from(argv)
    .map_err(|e| Whatever::without_source(e.to_string()))?;
  Ok((
    parsed.log,
    parsed.fst,
    parsed.trace,
    parsed.trace_mask,
    parsed.batch,
    parsed.help,
  ))
}
