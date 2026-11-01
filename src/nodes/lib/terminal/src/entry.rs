use crate::{cli, tui};

pub fn run() -> Result<(), String> {
    cli::parse().and_then(tui::run)
}
