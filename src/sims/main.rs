use snafu::Whatever;

struct BebopSim {
    elf: PathBuf,
    args: Vec<String>,
}

impl BebopSim {
    fn new(elf: PathBuf, args: Vec<String>) -> Self {
        Self { elf, args }
    }

    fn run(self) -> Result<(), Whatever> {

    }
}
