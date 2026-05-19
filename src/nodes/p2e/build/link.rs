use duct::cmd;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn build_cpp_wrapper(manifest_dir: &Path, out_dir: &Path) {
    let wrapper_src = manifest_dir.join("src/ctb/ctb_wrapper.cpp");
    let wrapper_obj = out_dir.join("ctb_wrapper.o");
    let wrapper_lib = out_dir.join("libctb_wrapper.a");

    println!("cargo:rerun-if-changed={}", wrapper_src.display());

    let include_dir = out_dir.join("vvacDir/runtimeDir/include");
    let include_arg = format!("-I{}", include_dir.display());

    // Why: must use VVAC's libstdc++ for ABI compatibility
    let vvac_lib_dir = out_dir.join("vvacDir/runtimeDir/lib/lib_arm");
    let vvac_libstdcxx = vvac_lib_dir.join("libstdc++.so.6");

    println!("cargo:warning=Compiling C++ wrapper include: {}", include_dir.display());
    println!("cargo:warning=Wrapper source: {}", wrapper_src.display());
    println!("cargo:warning=Output object: {}", wrapper_obj.display());
    println!("cargo:warning=Using VVAC's libstdc++: {}", vvac_libstdcxx.display());

    cmd!(
        "g++",
        "-c",
        "-fPIC",
        "-std=c++11",
        &include_arg,
        wrapper_src.to_str().unwrap(),
        "-o",
        wrapper_obj.to_str().unwrap()
    )
    .run()
    .expect("failed to compile C++ wrapper");

    cmd!(
        "ar",
        "rcs",
        wrapper_lib.to_str().unwrap(),
        wrapper_obj.to_str().unwrap()
    )
    .run()
    .expect("failed to create wrapper library");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ctb_wrapper");

    println!("cargo:rustc-link-search=native={}", vvac_lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

pub fn link_vvac(libctb: &Path) {
    let lib_dir = libctb.parent().expect("libvCtb.so parent directory");
    let lib_dir_str = lib_dir.display().to_string();

    println!("cargo:warning=Setting RPATH to: {}", lib_dir_str);
    println!("cargo:warning=libvCtb.so location: {}", libctb.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target_lib = out_dir.join("../../../libvCtb.so");
    if let Err(e) = fs::copy(libctb, &target_lib) {
        println!("cargo:warning=Failed to copy libvCtb.so to target: {}", e);
    } else {
        println!("cargo:warning=Copied libvCtb.so to {}", target_lib.display());
    }

    println!("cargo:rustc-link-search=native={}", lib_dir_str);
    println!("cargo:rustc-link-lib=dylib=vCtb");
    println!("cargo:rustc-link-lib=static=ctb_wrapper");

    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir_str);
    println!("cargo:rustc-link-arg=-Wl,--enable-new-dtags");

    // Why: lazy binding lets LD_PRELOAD override DPI-C symbols (scu_uart_write, scu_sim_exit)
    // at runtime, instead of failing at load time.
    println!("cargo:rustc-link-arg=-Wl,-z,lazy");

    // Why: libvCtb.so resolves DPI-C functions (scu_uart_write, scu_sim_exit) from the main
    // executable at runtime; without --export-dynamic those symbols are hidden.
    println!("cargo:rustc-link-arg=-Wl,--export-dynamic");

    println!("cargo:rustc-cfg=vvac_linked");
}
