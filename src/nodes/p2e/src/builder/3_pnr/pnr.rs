use std::path::PathBuf;
use std::process::Command;

/// PNR 布局布线步骤
pub struct PnrStep {
    /// 输出目录
    pub output_dir: PathBuf,
}

impl PnrStep {
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    /// 运行 PNR 布局布线
    pub fn run(&self) -> Result<PathBuf, String> {
        self.run_steps(false)
    }

    pub fn resume_post_route(&self) -> Result<PathBuf, String> {
        self.run_steps(true)
    }

    fn run_steps(&self, resume_post_route: bool) -> Result<PathBuf, String> {
        log::info!(
            "{}",
            if resume_post_route {
                "Resuming post-route processing..."
            } else {
                "Running PNR (Place and Route)..."
            }
        );

        let fpga_comp_dir = self.output_dir.join("fpgaCompDir");

        if !fpga_comp_dir.exists() {
            return Err(format!("fpgaCompDir not found: {:?}", fpga_comp_dir));
        }

        // Copy PNR_settings.tcl to output directory
        let pnr_settings_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/builder/3_pnr/PNR_settings.tcl");
        let pnr_settings_dst = self.output_dir.join("PNR_settings.tcl");

        std::fs::copy(&pnr_settings_src, &pnr_settings_dst)
            .map_err(|e| format!("Failed to copy PNR_settings.tcl: {}", e))?;

        // Source sourceme.sh and run make
        let sourceme_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
        let make_cmd = if resume_post_route {
            for name in ["xepic_vvac_top_0_0_route.dcp", "xepic_vvac_top_0_0.bit"] {
                let artifact = fpga_comp_dir.join("part_b0_f0/pnrDir").join(name);
                if !artifact.is_file() {
                    return Err(format!("Post-route resume requires {}", artifact.display()));
                }
            }
            "cd \"$1\" && source \"$2\" && make -j1 -C fpgaCompDir rerun_sta_collect sta_report readbackDB_create vdbg_loc_parser find_revise_net_name_all vdbg_gen_data_files"
        } else {
            r#"cd "$1" && source "$2" || exit 1
set -e
make -j1 -C fpgaCompDir clean
for part in fpgaCompDir/part_b*_f*; do
    name=${part##*/}
    make -j1 -C fpgaCompDir "syn_$name" > "$part/.syn.log" 2>&1
    make -j1 -C fpgaCompDir "pnr_$name" > "$part/.pnr.log" 2>&1
    id=${name#part_b}
    id=${id/_f/_}
    test -s "$part/pnrDir/xepic_vvac_top_$id.bit" || { echo "Bitstream missing for $name" >&2; exit 1; }
done
make -j1 -C fpgaCompDir post_pnr_summary sta_report readbackDB_create vdbg_loc_parser find_revise_net_name_all vdbg_gen_data_files"#
        };

        let status = Command::new("bash")
            .arg("-c")
            .arg(&make_cmd)
            .arg("p2e-build")
            .arg(&self.output_dir)
            .arg(&sourceme_path)
            .status()
            .map_err(|e| format!("Failed to execute make: {}", e))?;

        if !status.success() {
            return Err("PNR or post-route processing failed; inspect the partition logs".to_string());
        }

        // Copy bitstream from pnrDir to fpgaCompDir root
        let bitstream_src = self
            .output_dir
            .join("fpgaCompDir/part_b0_f0/pnrDir/xepic_vvac_top_0_0.bit");
        let bitstream_dst = self.output_dir.join("fpgaCompDir/bitstream.bit");

        std::fs::copy(&bitstream_src, &bitstream_dst).map_err(|e| format!("Failed to copy bitstream: {}", e))?;

        log::info!("PNR completed");
        log::info!("  Bitstream: {:?}", bitstream_dst);
        Ok(bitstream_dst)
    }
}
