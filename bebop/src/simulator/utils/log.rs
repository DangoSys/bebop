/// Global logging configuration
/// This module provides compatibility functions for the new log system.
/// The actual logging is handled by env_logger initialized in main().
use log::LevelFilter;
use std::io::Write;

pub fn init_log() {
  env_logger::Builder::new()
    .format(|buf, record| {
      let msg = format!("{}", record.args());
      if msg.starts_with('\n') {
        let msg_without_newline = &msg[1..];
        writeln!(buf, "\n\x1b[34m[log]\x1b[0m {}", msg_without_newline)
      } else {
        writeln!(buf, "\x1b[34m[log]\x1b[0m {}", msg)
      }
    })
    .filter(None, LevelFilter::Info)
    .init();
}

/// Set logging enabled/disabled
/// This is a compatibility function that maps to log::set_max_level
pub fn set_log(enabled: bool) {
  if enabled {
    log::set_max_level(LevelFilter::Info);
  } else {
    log::set_max_level(LevelFilter::Off);
  }
}

/// Check if logging is enabled
/// This checks if the current log level allows info messages
pub fn is_log_enabled() -> bool {
  log::max_level() >= LevelFilter::Info
}
