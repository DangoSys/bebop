use std::path::PathBuf;

fn main() {
    let proto = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../../../../bbdev/api/steps/config/scripts/proto");
    prost_build::compile_protos(&[proto.join("design.proto")], &[&proto]).unwrap();
    println!("cargo:rerun-if-changed={}", proto.display());
}
