use clap::Parser;

fn main() {
    if let Err(error) =
        bebop_bemu::sim::run::run(bebop_bemu::sim::run::Args::parse(), &simulation::SIMULATION)
    {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
