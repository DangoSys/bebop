mod app;
mod cli;
mod conn;
mod form;
mod tui;

pub fn run() -> Result<(), String> {
    cli::parse().and_then(tui::run)
}
