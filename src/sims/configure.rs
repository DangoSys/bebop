pub fn configure(design: &str) -> Result<(), snafu::Whatever> {
    use snafu::ResultExt;
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/config.py");
    duct::cmd!("python3", script, "--design", design)
        .run()
        .whatever_context("failed to install Buckyball design configuration")?;
    Ok(())
}
