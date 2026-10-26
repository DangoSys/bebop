fn main() {
    if let Err(error) = bebop_trace_perfetto::run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
