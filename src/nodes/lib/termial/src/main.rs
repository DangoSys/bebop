mod app;
mod cli;
mod conn;
mod form;
mod tui;

fn main() {
  if let Err(e) = cli::parse().and_then(tui::run) {
    eprintln!("Error: {e}");
    std::process::exit(1);
  }
}
