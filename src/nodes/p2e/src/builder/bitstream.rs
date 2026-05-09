use std::path::PathBuf;

use super::pnr::PnrStep;
use super::vcom::VcomStep;
use super::vsyn::VsynStep;

/// Bitstream builder
pub struct BitstreamBuilder {
    vsrc_dir: PathBuf,
    output_dir: PathBuf,
}

impl BitstreamBuilder {
    pub fn new(vsrc_dir: PathBuf, output_dir: PathBuf) -> Self {
        Self { vsrc_dir, output_dir }
    }

    pub fn build(&self) -> Result<(), String> {
        log::info!("Starting P2E bitstream build...");

        self.setup_environment()?;

        std::fs::create_dir_all(&self.output_dir)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;

        self.verify_vvac_outputs()?;

        // Step 1: vsyn
        let vsyn = VsynStep::new(self.output_dir.clone(), "xepic_vvac_top".to_string());
        vsyn.run()?;

        // Step 2: vcom
        let vcom_tcl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/builder/2_vcom/vcom_compile.tcl");
        let vcom = VcomStep::new(
            self.output_dir.clone(),
            "xepic_vvac_top".to_string(),
            vcom_tcl,
        )?;
        vcom.run()?;

        // Step 3: PNR
        let pnr = PnrStep::new(self.output_dir.clone());
        pnr.run()?;

        log::info!("P2E bitstream build completed successfully");
        log::info!("Bitstream: {:?}", self.bitstream_path());
        log::info!("libvCtb.so: {:?}", self.libvctb_path());
        Ok(())
    }

    fn setup_environment(&self) -> Result<(), String> {
        let sourceme_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");

        if !sourceme_path.exists() {
            return Err(format!("sourceme.sh not found at {:?}", sourceme_path));
        }

        log::info!("Environment will be sourced from {:?} in each step", sourceme_path);
        Ok(())
    }

    fn verify_vvac_outputs(&self) -> Result<(), String> {
        let vvac_dir = self.output_dir.join("vvacDir");
        if !vvac_dir.exists() {
            return Err(format!(
                "vvacDir not found at {}; build p2e with VSRC_PATH first",
                vvac_dir.display()
            ));
        }

        let libvctb = self.libvctb_path();
        if !libvctb.exists() {
            return Err(format!(
                "libvCtb.so not found at {}; build p2e with VSRC_PATH first",
                libvctb.display()
            ));
        }

        Ok(())
    }

    pub fn bitstream_path(&self) -> PathBuf {
        self.output_dir.join("fpgaCompDir/bitstream.bit")
    }

    pub fn libvctb_path(&self) -> PathBuf {
        self.output_dir.join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so")
    }

    pub fn rtcfg_path(&self) -> PathBuf {
        self.output_dir.join("vvacDir/runtimeDir/rtcfg")
    }
}
