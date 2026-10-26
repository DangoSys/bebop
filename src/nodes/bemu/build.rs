use std::{env, fs, path::PathBuf};

// The design schema and every Ball's parameter schema. Which design runs, and
// which Balls it has, is decided by the simulation entry that links this crate.
fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let repo = manifest_dir.join("../../../..");
    let proto_dir = repo.join("bbdev/api/steps/config/scripts/proto");
    let balls_dir = repo.join("examples/components/balls");
    let mut protos = vec![proto_dir.join("design.proto")];
    let mut includes = vec![proto_dir.clone()];
    let mut modules = String::new();
    let mut components = fs::read_dir(&balls_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    components.sort();
    for component in components {
        let name = component.file_name().unwrap().to_str().unwrap();
        let schema = component.join("configs").join(format!("{name}_params.proto"));
        if schema.is_file() {
            includes.push(schema.parent().unwrap().to_path_buf());
            protos.push(schema);
            modules.push_str(&format!(
                "pub mod {name} {{ include!(concat!(env!(\"OUT_DIR\"), \"/buckyball.balls.{name}.rs\")); }}\n"
            ));
        }
    }
    prost_build::compile_protos(&protos, &includes).unwrap();
    fs::write(out_dir.join("ball_parameters.rs"), modules).unwrap();
    println!("cargo:rerun-if-changed={}", balls_dir.display());
    println!("cargo:rerun-if-changed={}", proto_dir.display());
}
