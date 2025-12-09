/// Global logging configuration
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flags for controlling log output
static ENABLE_EVENT_LOG: AtomicBool = AtomicBool::new(true);
static ENABLE_FORWARD_LOG: AtomicBool = AtomicBool::new(true);
static ENABLE_BACKWARD_LOG: AtomicBool = AtomicBool::new(true);

/// Enable or disable event logging
pub fn set_event_log(enabled: bool) {
    ENABLE_EVENT_LOG.store(enabled, Ordering::Relaxed);
}

/// Enable or disable forward logging
pub fn set_forward_log(enabled: bool) {
    ENABLE_FORWARD_LOG.store(enabled, Ordering::Relaxed);
}

/// Enable or disable backward logging
pub fn set_backward_log(enabled: bool) {
    ENABLE_BACKWARD_LOG.store(enabled, Ordering::Relaxed);
}

/// Check if event logging is enabled
pub fn is_event_log_enabled() -> bool {
    ENABLE_EVENT_LOG.load(Ordering::Relaxed)
}

/// Check if forward logging is enabled
pub fn is_forward_log_enabled() -> bool {
    ENABLE_FORWARD_LOG.load(Ordering::Relaxed)
}

/// Check if backward logging is enabled
pub fn is_backward_log_enabled() -> bool {
    ENABLE_BACKWARD_LOG.load(Ordering::Relaxed)
}

