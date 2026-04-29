use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use crate::types::TclOut;

pub(crate) fn run_file(
  bin: impl AsRef<OsStr>,
  args: &[&str],
  file: impl AsRef<Path>,
) -> Result<TclOut, String> {
  let output = Command::new(bin)
    .args(args)
    .arg(file.as_ref())
    .output()
    .map_err(|e| format!("failed to run tcl file: {e}"))?;

  if !output.status.success() {
    return Err(format!(
      "tcl file failed with status {:?}: {}",
      output.status.code(),
      String::from_utf8_lossy(&output.stderr)
    ));
  }

  Ok(TclOut {
    out: String::from_utf8(output.stdout).map_err(|e| format!("stdout is not utf8: {e}"))?,
    err: String::from_utf8(output.stderr).map_err(|e| format!("stderr is not utf8: {e}"))?,
  })
}
