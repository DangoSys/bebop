use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Only set RPATH when building with p2e feature
    if env::var("CARGO_FEATURE_P2E").is_ok() {
        let bebop_root = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let libvctb_dir = format!("{}/out", bebop_root);

        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", libvctb_dir);
        println!("cargo:rustc-link-arg=-Wl,--enable-new-dtags");

        println!("cargo:warning=Setting RPATH for bebop main binary to: {}", libvctb_dir);
    }
}
