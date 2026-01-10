/// Global logging configuration
use std::sync::atomic::{AtomicBool, Ordering};
static ENABLE_LOG: AtomicBool = AtomicBool::new(true);

/// Set logging enabled
pub fn set_log(enabled: bool) {
  ENABLE_LOG.store(enabled, Ordering::Relaxed);
}

/// Check if logging is enabled, default is true
pub fn is_log_enabled() -> bool {
  ENABLE_LOG.load(Ordering::Relaxed)
}

/// Print a log message with blue [Log] prefix
#[macro_export]
macro_rules! log_info {
  ($($arg:tt)*) => {
    println!("\x1b[34m[Log]\x1b[0m {}", format!($($arg)*));
  };
}
