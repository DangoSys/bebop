/// Generate a minimal device tree blob (DTB) for Linux kernel
///
/// This generates a very simple DTB with:
/// - Memory node
/// - CPU node
/// - Chosen node (for bootargs and initrd)

mod constants;
mod builder;

pub use constants::*;
pub use builder::*;
