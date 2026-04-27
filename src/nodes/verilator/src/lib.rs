mod config;
#[path = "main.rs"]
mod main_impl;

pub use main_impl::{run, VerilatorCli};
