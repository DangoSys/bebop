fn main() {
    if let Err(error) = bebop_terminal::run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
