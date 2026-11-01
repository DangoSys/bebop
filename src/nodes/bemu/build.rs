use std::{env, path::PathBuf};

#[path = "../../paths.rs"]
mod paths;

// The design schema. The parameter schema of each Ball is compiled by the
// simulation entry that links the Ball's model.
fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = paths::buckyball(&manifest_dir.join("../../.."));
    let proto_dir = repo.join("bbdev/api/steps/config/scripts/proto");
    prost_build::compile_protos(&[proto_dir.join("design.proto")], &[&proto_dir]).unwrap();
    println!("cargo:rerun-if-changed={}", proto_dir.display());
}
