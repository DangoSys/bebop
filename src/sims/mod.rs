pub mod build;
#[cfg(feature = "bemu")]
pub mod lib;
pub mod p2e;
pub mod run;
pub mod verilator;

pub use build::build;
pub use run::run;

pub fn configure(design: &str) -> Result<(), snafu::Whatever> {
    use snafu::ResultExt;
    let script = std::env::current_dir()
        .whatever_context("failed to locate Bebop checkout")?
        .join("scripts/config.py");
    duct::cmd!("python3", script, "--design", design)
        .run()
        .whatever_context("failed to install Buckyball design configuration")?;
    Ok(())
}
