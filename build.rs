use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=OUT_PATH");

    // Only set RPATH when building with p2e feature
    if env::var("CARGO_FEATURE_P2E").is_ok() {
        let bebop_root = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");

        // OUT_PATH overrides default location (matches build.rs in p2e crate)
        let out_dir = env::var("OUT_PATH").unwrap_or_else(|_| format!("{}/out", bebop_root));

        let libvctb_dir = out_dir.clone();

        // CRITICAL: Add VVAC's lib directory to RPATH to ensure ABI compatibility
        // VVAC libraries (libtbppeer.so, etc.) require libstdc++.so.6.0.25
        // which is provided in vvacDir/runtimeDir/lib/lib_arm/
        let vvac_lib_dir = format!("{}/vvacDir/runtimeDir/lib/lib_arm", out_dir);

        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vvac_lib_dir);
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", libvctb_dir);
        println!("cargo:rustc-link-arg=-Wl,--enable-new-dtags");

        println!("cargo:warning=Setting RPATH for bebop main binary to: {}", libvctb_dir);
        println!("cargo:warning=Adding VVAC lib directory to RPATH: {}", vvac_lib_dir);
    }
}
