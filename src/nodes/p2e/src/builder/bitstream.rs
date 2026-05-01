use crate::config::BitstreamConfig;
use std::path::PathBuf;
use std::process::Command;

use super::vsyn::VsynStep;
use super::vcom::VcomStep;
use super::pnr::PnrStep;

/// 比特流构建器
pub struct BitstreamBuilder {
    config: BitstreamConfig,
}

impl BitstreamBuilder {
    /// build new bitstream builder from config
    pub fn new(config: BitstreamConfig) -> Self {
        Self { config }
    }

    /// build full build process
    pub fn build(&self) -> Result<(), String> {
        log::info!("Starting P2E bitstream build...");

        self.setup_environment()?;

        // Create output directory
        std::fs::create_dir_all(&self.config.output_dir)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;

        self.verify_vvac_outputs()?;

        // Step 1: vsyn
        let vsyn = VsynStep::new(
            self.config.output_dir.clone(),
            self.config.vvac_top_module.clone(),
        );
        vsyn.run()?;

        // Step 2: vcom
        let vcom = VcomStep::new(
            self.config.output_dir.clone(),
            self.config.vvac_top_module.clone(),
            self.config.vcom_tcl.clone(),
        )?;
        vcom.run()?;

        // Step 3: PNR 
        let pnr = PnrStep::new(self.config.output_dir.clone());
        pnr.run()?;

        log::info!("P2E bitstream build completed successfully");
        log::info!("Bitstream: {:?}", self.bitstream_path());
        log::info!("libvCtb.so: {:?}", self.libvctb_path());
        Ok(())
    }

    fn setup_environment(&self) -> Result<(), String> {
        // Verify sourceme.sh exists
        let sourceme_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("sourceme.sh");

        if !sourceme_path.exists() {
            return Err(format!("sourceme.sh not found at {:?}", sourceme_path));
        }

        log::info!("Environment will be sourced from {:?} in each step", sourceme_path);
        Ok(())
    }

    fn verify_vvac_outputs(&self) -> Result<(), String> {
        let vvac_dir = self.config.output_dir.join("vvacDir");
        if !vvac_dir.exists() {
            return Err(format!(
                "vvacDir not found at {}; build p2e with ARCH_CONFIG first",
                vvac_dir.display()
            ));
        }

        let libvctb = self.libvctb_path();
        if !libvctb.exists() {
            return Err(format!(
                "libvCtb.so not found at {}; build p2e with ARCH_CONFIG first",
                libvctb.display()
            ));
        }

        Ok(())
    }

    /// get bitstream path
    pub fn bitstream_path(&self) -> PathBuf {
        self.config.output_dir.join("fpgaCompDir/bitstream.bit")
    }

    /// get libvCtb.so path
    pub fn libvctb_path(&self) -> PathBuf {
        self.config
            .output_dir
            .join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so")
    }

    /// get runtime configuration path
    pub fn rtcfg_path(&self) -> PathBuf {
        self.config.output_dir.join("vvacDir/runtimeDir/rtcfg")
    }
}
