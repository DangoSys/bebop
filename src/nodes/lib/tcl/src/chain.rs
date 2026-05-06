use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::exec::run_file;
use crate::types::TclOut;

pub fn run_dyn_chain(
  bin: impl AsRef<OsStr>,
  args: &[&str],
  dir: impl AsRef<Path>,
  pre: &str,
  nxt: &str,
) -> Result<TclOut, String> {
  let ts = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|e| format!("invalid system time: {e}"))?
    .as_nanos();
  let pid = std::process::id();
  let base = dir
    .as_ref()
    .canonicalize()
    .map_err(|e| format!("failed to resolve dir: {e}"))?;

  let next = base.join(format!("dyn_next_{pid}_{ts}.tcl"));
  let main = base.join(format!("dyn_main_{pid}_{ts}.tcl"));

  fs::write(&next, nxt).map_err(|e| format!("failed to write next script: {e}"))?;
  let next_s = next
    .to_str()
    .ok_or_else(|| "next script path is not utf8".to_string())?;
  let main_src = format!("{pre}\nsource {{{next_s}}}\n");
  fs::write(&main, main_src).map_err(|e| format!("failed to write main script: {e}"))?;

  run_file(bin, args, main)
}
