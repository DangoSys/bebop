use std::env;
use std::process::Command;

fn main() {
    // 链接芯华章 VVAC 库
    if let Ok(vvac_lib) = env::var("VVAC_LIB_DIR") {
        println!("cargo:rustc-link-search={}", vvac_lib);
        println!("cargo:rustc-link-lib=dylib=vCtb");
    }

    // feature flag 控制比特流生成
    #[cfg(feature = "build-bitstream")]
    {
        let verilog_dir = env::var("BEBOP_VERILOG_DIR")
            .unwrap_or_else(|_| "/home/wanghui/Code/buckyball/arch/build".to_string());

        println!("cargo:warning=Generating bitstream from {}", verilog_dir);

        // 调用 Vivado TCL 脚本
        let status = Command::new("vivado")
            .args(&["-mode", "batch", "-source", "vcom_compile.tcl"])
            .env("VERILOG_DIR", verilog_dir)
            .status()
            .expect("Failed to run Vivado");

        assert!(status.success(), "Bitstream generation failed");
    }

    println!("cargo:rerun-if-env-changed=VVAC_LIB_DIR");
    println!("cargo:rerun-if-env-changed=BEBOP_VERILOG_DIR");
}
