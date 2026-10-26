fn main() {
    if let Err(error) = bebop_vm::run() {
        eprintln!("VM: {error}");
        std::process::exit(1);
    }
}
