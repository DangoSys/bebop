use std::path::PathBuf;

use super::pnr::PnrStep;
use super::vcom::VcomStep;
use super::vsyn::VsynStep;

#[derive(Debug)]
pub enum BuildOutcome {
    Assessment(&'static str),
    Runtime,
}

/// Bitstream builder
///
/// `build_dir` contains both the design build artifacts (vvacDir) and synthesis outputs.
/// All intermediate files (vsyn, vcom, pnr) and final bitstream are generated under `build_dir`.
pub struct BitstreamBuilder {
    build_dir: PathBuf,
}

impl BitstreamBuilder {
    pub fn new(build_dir: PathBuf) -> Self {
        Self { build_dir }
    }

    pub fn build(&self, stop_after: Option<&str>) -> Result<BuildOutcome, String> {
        if !matches!(stop_after, None | Some("vsyn") | Some("vcom")) {
            return Err("stop-after must be vsyn or vcom".to_string());
        }
        if stop_after.is_some() && (self.bitstream_path().exists() || self.build_dir.join("bebop-p2e").exists()) {
            return Err("Resource assessment requires a case without an existing bitstream/runtime".to_string());
        }
        log::info!("Starting P2E bitstream build...");
        log::info!("  Build dir: {:?}", self.build_dir);

        self.setup_environment()?;

        std::fs::create_dir_all(&self.build_dir).map_err(|e| format!("Failed to create build directory: {}", e))?;

        self.verify_vvac_outputs()?;

        // Step 1: vsyn
        let vsyn = VsynStep::new(self.build_dir.clone(), "xepic_vvac_top".to_string());
        vsyn.run()?;
        if stop_after == Some("vsyn") {
            return Ok(BuildOutcome::Assessment("vsyn"));
        }

        // Step 2: vcom
        let vcom_tcl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/builder/2_vcom/vcom_compile.tcl");
        let vcom = VcomStep::new(self.build_dir.clone(), "xepic_vvac_top".to_string(), vcom_tcl)?;
        vcom.run()?;
        if stop_after == Some("vcom") {
            return Ok(BuildOutcome::Assessment("vcom"));
        }

        // Step 3: PNR
        let pnr = PnrStep::new(self.build_dir.clone());
        pnr.run()?;

        log::info!("P2E bitstream build completed successfully");
        log::info!("Bitstream: {:?}", self.bitstream_path());
        log::info!("libvCtb.so: {:?}", self.libvctb_path());
        Ok(BuildOutcome::Runtime)
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
        let vvac_dir = self.build_dir.join("vvacDir");
        if !vvac_dir.exists() {
            return Err(format!(
                "vvacDir not found at {}; build p2e with VSRC_PATH and OUT_PATH first",
                vvac_dir.display()
            ));
        }

        let libvctb = self.libvctb_path();
        if !libvctb.exists() {
            return Err(format!(
                "libvCtb.so not found at {}; build p2e with VSRC_PATH and OUT_PATH first",
                libvctb.display()
            ));
        }

        Ok(())
    }

    pub fn bitstream_path(&self) -> PathBuf {
        self.build_dir.join("fpgaCompDir/bitstream.bit")
    }

    pub fn libvctb_path(&self) -> PathBuf {
        self.build_dir.join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so")
    }

    pub fn rtcfg_path(&self) -> PathBuf {
        self.build_dir.join("vvacDir/runtimeDir/rtcfg")
    }
}
