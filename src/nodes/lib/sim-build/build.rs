use std::path::PathBuf;

#[path = "../../../paths.rs"]
mod paths;

fn main() {
    let bebop = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../../..");
    let proto = paths::buckyball(&bebop).join("bbdev/api/steps/config/scripts/proto");
    prost_build::compile_protos(&[proto.join("design.proto")], &[&proto]).unwrap();
    println!("cargo:rerun-if-changed={}", proto.display());
}
