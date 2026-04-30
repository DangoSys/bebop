use bebop_p2e::{parse_args, BitstreamBuilder, BitstreamConfig};
use std::path::PathBuf;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if let Some(arch_config) = args.buildbitstream {
        log::info!("Building bitstream for config: {}", arch_config);

        // 查找 vcom_compile.tcl
        let vcom_tcl = find_vcom_tcl().unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        });

        let config = BitstreamConfig::new(&arch_config, vcom_tcl)
            .output_dir("./out")
            .hw_config("./hw-config.hdf");

        let builder = BitstreamBuilder::new(config);

        if let Err(e) = builder.build() {
            eprintln!("Build failed: {}", e);
            std::process::exit(1);
        }

        log::info!("Bitstream build completed successfully");
    }

    if args.runworkload {
        log::info!("Running workload...");
        // TODO: implement runworkload
        eprintln!("runworkload not yet implemented");
        std::process::exit(1);
    }
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
