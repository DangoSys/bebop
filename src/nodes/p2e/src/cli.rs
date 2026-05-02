use crate::config::BitstreamConfig;
use crate::builder::BitstreamBuilder;
use std::path::PathBuf;
use snafu::{Whatever, FromString};

#[derive(Debug, Clone)]
pub struct P2ECli {
    pub buildbitstream: bool,
    pub runworkload: bool,
    pub config: Option<String>,
    pub image: Option<PathBuf>,
    pub bitstream: Option<PathBuf>,
}

pub fn run(cli: P2ECli) -> Result<(), Whatever> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if cli.buildbitstream {
        log::info!("Building bitstream...");

        // Get ARCH_CONFIG from CLI argument or environment variable
        let arch_config = cli.config
            .or_else(|| std::env::var("ARCH_CONFIG").ok())
            .ok_or_else(|| Whatever::without_source(
                "Architecture config not specified. Use --config or set ARCH_CONFIG environment variable".to_string()
            ))?;

        log::info!("Building bitstream for config: {}", arch_config);

        // 查找 vcom_compile.tcl
        let vcom_tcl = find_vcom_tcl()
            .map_err(|e| Whatever::without_source(e))?;

        let config = BitstreamConfig::new(&arch_config, vcom_tcl)
            .output_dir("./out")
            .hw_config("./hw-config.hdf");

        let builder = BitstreamBuilder::new(config);

        builder.build()
            .map_err(|e| Whatever::without_source(format!("Build failed: {}", e)))?;

        log::info!("Bitstream build completed successfully");
    }

    if cli.runworkload {
        log::info!("Running workload...");

        let _image = cli.image.ok_or_else(|| Whatever::without_source(
            "Workload image not specified. Use --image".to_string()
        ))?;

        let _bitstream = cli.bitstream.ok_or_else(|| Whatever::without_source(
            "Bitstream not specified. Use --bitstream".to_string()
        ))?;

        // TODO: implement runworkload
        return Err(Whatever::without_source("runworkload not yet implemented".to_string()));
    }

    Ok(())
}

fn find_vcom_tcl() -> Result<PathBuf, String> {
    // 1. 从源代码目录查找（仓库中的配置文件）
    let src_tcl = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/builder/2_vcom/vcom_compile.tcl");
    if src_tcl.exists() {
        return Ok(src_tcl);
    }

    // 2. 在当前目录查找
    let tcl = PathBuf::from("vcom_compile.tcl");
    if tcl.exists() {
        return Ok(tcl);
    }

    // 3. 在 out 目录查找
    let tcl = PathBuf::from("./out/vcom_compile.tcl");
    if tcl.exists() {
        return Ok(tcl);
    }

    Err("vcom_compile.tcl not found. Please provide vcom_compile.tcl in current directory or out/ directory".to_string())
}
