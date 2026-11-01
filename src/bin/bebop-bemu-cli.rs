fn main() {
    if let Err(error) = bebop_cli::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
