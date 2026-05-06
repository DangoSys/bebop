use snafu::{whatever, ResultExt, Whatever};

pub fn parse_args(
  args: Vec<String>,
) -> Result<(String, String, usize, String, bool, bool), Whatever> {
  let mut log = String::from("bemu.log");
  let mut isa = String::from("rv64gc");
  let mut procs = 1;
  let mut mem_size = String::from("2048");
  let mut batch = false;
  let mut help = false;

  let mut i = 0;
  while i < args.len() {
    match args[i].as_str() {
      "--log" => {
        i += 1;
        if i >= args.len() {
          whatever!("--log requires an argument");
        }
        log = args[i].clone();
      }
      "--isa" => {
        i += 1;
        if i >= args.len() {
          whatever!("--isa requires an argument");
        }
        isa = args[i].clone();
      }
      "-p" | "--procs" => {
        i += 1;
        if i >= args.len() {
          whatever!("--procs requires an argument");
        }
        procs = args[i].parse().whatever_context("invalid procs value")?;
      }
      "-m" | "--mem" => {
        i += 1;
        if i >= args.len() {
          whatever!("--mem requires an argument");
        }
        mem_size = args[i].clone();
      }
      "--batch" => {
        batch = true;
      }
      "--help" | "-h" => {
        help = true;
      }
      _ => {
        whatever!("unknown argument: {}", args[i]);
      }
    }
    i += 1;
  }

  Ok((log, isa, procs, mem_size, batch, help))
}
