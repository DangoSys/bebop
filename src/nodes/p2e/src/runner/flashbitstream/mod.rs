use std::path::{Path, PathBuf};
use std::process::Command;

/// Flash bitstream to FPGA using Vivado hw_server
pub struct FlashBitstreamStep {
    pub bitstream_path: PathBuf,
    pub hw_server_url: String,
    pub fpga_part: String,
}

impl FlashBitstreamStep {
    pub fn new(bitstream_path: impl Into<PathBuf>) -> Self {
        Self {
            bitstream_path: bitstream_path.into(),
            hw_server_url: "localhost:3121".to_string(),
            fpga_part: "xcvu19p_0".to_string(),
        }
    }

    pub fn hw_server(mut self, url: impl Into<String>) -> Self {
        self.hw_server_url = url.into();
        self
    }

    pub fn fpga_part(mut self, part: impl Into<String>) -> Self {
        self.fpga_part = part.into();
        self
    }

    pub fn run(&self) -> Result<(), String> {
        log::info!("Flashing bitstream to FPGA...");
        log::info!("  Bitstream: {:?}", self.bitstream_path);
        log::info!("  HW Server: {}", self.hw_server_url);
        log::info!("  FPGA Part: {}", self.fpga_part);

        if !self.bitstream_path.exists() {
            return Err(format!(
                "Bitstream not found: {}",
                self.bitstream_path.display()
            ));
        }

        // Generate TCL script for flashing
        let tcl_script = self.generate_flash_tcl();
        let tcl_path = PathBuf::from("/tmp/flash_bitstream.tcl");
        std::fs::write(&tcl_path, tcl_script)
            .map_err(|e| format!("Failed to write TCL script: {}", e))?;

        // Run Vivado in batch mode
        let status = Command::new("vivado")
            .args(&[
                "-mode", "batch",
                "-source", tcl_path.to_str().unwrap(),
            ])
            .status()
            .map_err(|e| format!("Failed to execute Vivado: {}", e))?;

        if !status.success() {
            return Err("Bitstream flashing failed".to_string());
        }

        log::info!("Bitstream flashed successfully");
        Ok(())
    }

    fn generate_flash_tcl(&self) -> String {
        format!(
            r#"
# Connect to hw_server
open_hw_manager
connect_hw_server -url {}

# Open target
current_hw_target [get_hw_targets */xilinx_tcf/Xilinx/*]
open_hw_target

# Set device
current_hw_device [get_hw_devices {}]
refresh_hw_device -update_hw_probes false [lindex [get_hw_devices {}] 0]

# Program device
set_property PROGRAM.FILE {{{}}} [get_hw_devices {}]
program_hw_devices [get_hw_devices {}]

# Close
close_hw_target
disconnect_hw_server
close_hw_manager
"#,
            self.hw_server_url,
            self.fpga_part,
            self.fpga_part,
            self.bitstream_path.display(),
            self.fpga_part,
            self.fpga_part
        )
    }
}
