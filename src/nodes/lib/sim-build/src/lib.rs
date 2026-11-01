//! Build support for simulation entries: what a design.pb asks the entry to link,
//! and how the runners the entry builds are linked.
#[path = "../../../../paths.rs"]
mod paths;

mod config;
mod simulation;

pub use simulation::{balls, runner_links, runner_manifest};
