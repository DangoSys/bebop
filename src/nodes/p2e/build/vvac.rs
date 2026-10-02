use duct::cmd;
use std::fs;
use std::path::Path;

pub fn run_vvac(out_dir: &Path, sourceme: &Path, flist: &Path, top: &str, mode: &str) {
    let trace_args = if mode != "none" {
        fs::write(
            out_dir.join("p2e_trace_functions.cfg"),
            match mode {
                "difftest-n" => "module: BTraceDPI\nfunction: dpi_btrace\nchannel: vc_default\ntype: nb\n",
                "access" => "module: AccessWriteDPI\nfunction: dpi_access_write\nchannel: vc_default\ntype: nb\n",
                _ => unreachable!(),
            },
        )
        .expect("write nonblocking BTrace configuration");
        " -tf_cfg p2e_trace_functions.cfg"
    } else {
        ""
    };
    let vvac_cmd = format!(
        r#"clang_format_bin="$(command -v clang-format || true)"
clang_format_bin="${{clang_format_bin%/*}}"
source {sourceme}
if [ -z "$HPEC_HOME" ]; then
    echo "HPEC_HOME not set after sourceme" >&2
    exit 1
fi
cc_bin="$HPEC_HOME/tools/gcc-8.3.0/gcc-8.3.0/bin/gcc"
cxx_bin="$HPEC_HOME/tools/gcc-8.3.0/gcc-8.3.0/bin/g++"
if [ ! -x "$cc_bin" ] || [ ! -x "$cxx_bin" ]; then
    echo "HPE gcc not found: $cc_bin $cxx_bin" >&2
    exit 1
fi
vvac_bin="$(command -v vvac || true)"
vvac_bin="${{vvac_bin%/*}}"
filtered_path=()
IFS=: read -r -a path_entries <<< "$PATH"
for path_entry in "${{path_entries[@]}}"; do
    case "$path_entry" in
        "$clang_format_bin"|"$vvac_bin"|/home/wanghui/Code/buckyball/result/bin|/usr/*|/bin|/sbin) ;;
        *) filtered_path+=("$path_entry") ;;
    esac
done
PATH="$(IFS=:; printf '%s' "${{filtered_path[*]}}")"
PATH="${{vvac_bin}}:$PATH"
export PATH
unset CMAKE_C_COMPILER CMAKE_CXX_COMPILER NIX_CC
export CC="$cc_bin"
export CXX="$cxx_bin"
vvac -bc -f {flist} -top {top}{trace_args}"#,
        sourceme = sourceme.display(),
        flist = flist.display(),
        top = top,
        trace_args = trace_args,
    );

    cmd!("bash", "-c", &vvac_cmd)
        .dir(out_dir)
        .stdout_to_stderr()
        .run()
        .unwrap_or_else(|e| {
            panic!(
                "vvac failed: {}. Check log: {}",
                e,
                out_dir.join("vvac_build.log").display()
            )
        });
    verify_trace(out_dir, mode);
}

fn db_value<'a>(block: &'a str, name: &str) -> &'a str {
    block
        .lines()
        .find_map(|line| line.trim().strip_prefix(name).map(str::trim))
        .unwrap_or_else(|| panic!("missing VVAC database field {name}"))
}

pub fn verify_trace(out_dir: &Path, mode: &str) {
    let db = fs::read_to_string(out_dir.join("vvacDir/db.txt")).expect("read VVAC database");
    let declarations = db
        .split("channel.db :")
        .next()
        .unwrap()
        .split("  func_name : ")
        .skip(1)
        .collect::<Vec<_>>();
    let scope_section = db
        .split("scope.db :")
        .nth(1)
        .unwrap()
        .split("funcCall.db :")
        .next()
        .unwrap();
    let scopes = scope_section
        .split("  name : ")
        .skip(1)
        .map(|block| block.lines().next().unwrap())
        .collect::<Vec<_>>();
    let calls = db.split("funcCall.db :").nth(1).unwrap();
    for forbidden in match mode {
        "none" => &["dpi_btrace", "btrace_snapshot", "dpi_access_write", "access_snapshot"][..],
        "access" => &["dpi_btrace", "btrace_snapshot"][..],
        "difftest-n" => &["dpi_access_write", "access_snapshot"][..],
        _ => panic!("invalid verification mode"),
    } {
        assert!(
            !declarations
                .iter()
                .any(|block| block.lines().next() == Some(*forbidden)),
            "unexpected verification callback {forbidden} in {mode}"
        );
    }
    let mut functions = vec![("execution_snapshot", "p2e_execution_scopes", false)];
    match mode {
        "none" => {}
        "access" => {
            functions.push(("dpi_access_write", "", true));
            functions.push(("access_snapshot", "p2e_access_scopes", false));
        }
        "difftest-n" => {
            functions.push(("dpi_btrace", "", true));
            functions.push(("btrace_snapshot", "p2e_btrace_scopes", false));
        }
        _ => panic!("invalid verification mode"),
    }
    for (name, output, nonblocking) in functions {
        let id = declarations
            .iter()
            .position(|block| block.lines().next() == Some(name))
            .unwrap_or_else(|| panic!("missing VVAC function {name}"));
        if nonblocking {
            assert_eq!(db_value(declarations[id], "is_nb :"), "1", "{name} must be nonblocking");
        }
        let mut instances = Vec::new();
        for call in calls.split("  decl_id : ").skip(1) {
            if call.lines().next().unwrap().parse::<usize>().unwrap() != id {
                continue;
            }
            if nonblocking {
                assert_eq!(db_value(call, "work_mode :"), "1", "{name} must be nonblocking");
            }
            let scope_id = db_value(call, "scope_id :").parse::<usize>().unwrap();
            instances.push(scopes[scope_id]);
        }
        assert!(!instances.is_empty(), "no VVAC instances of {name}");
        if !output.is_empty() {
            fs::write(out_dir.join(output), instances.join("\n") + "\n").unwrap();
        }
    }
}

pub fn add_missing_empty_modules(out_dir: &Path) -> bool {
    let filelist_path = out_dir.join("vvacDir/vvac_by_mod/filelist");
    if !filelist_path.exists() {
        println!("cargo:warning=VVAC filelist not found, skipping");
        return false;
    }

    let content = fs::read_to_string(&filelist_path).expect("Failed to read VVAC filelist");
    let vvac_dir = out_dir.join("vvacDir/vvac_by_mod");

    let empty_modules = [
        "work_DebugCustomXbar.sv",
        "work_IntSyncCrossingSource_n1x1_Registered.sv",
        "work_NullIntSource.sv",
        "work_SourceX.sv",
        "work_Queue1_SourceXRequest.sv",
    ];

    let mut added_count = 0;
    let mut new_lines = Vec::new();

    for module in &empty_modules {
        let file_path = vvac_dir.join(module);

        if file_path.exists() && !content.contains(module) {
            new_lines.push(format!("./{}", module));
            println!("cargo:warning=Adding missing empty module to filelist: {}", module);
            added_count += 1;
        }
    }

    if added_count == 0 {
        println!("cargo:warning=No missing empty modules found");
        return false;
    }

    let mut new_content = content;
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(&new_lines.join("\n"));
    new_content.push('\n');

    fs::write(&filelist_path, new_content).expect("Failed to write updated VVAC filelist");
    println!("cargo:warning=Added {} empty modules to VVAC filelist", added_count);
    true
}

pub fn remove_empty_module_instantiations(build_dir: &Path) {
    let empty_modules = [
        "IntSyncCrossingSource_n1x1_Registered",
        "NullIntSource",
        "IntXbar_i0_o0",
        "SourceX",
        "Queue1_SourceXRequest",
    ];

    let mut total_removed = 0;
    let entries = fs::read_dir(build_dir).expect("Failed to read Verilog source directory");
    for entry in entries {
        let path = entry.expect("Failed to read Verilog source entry").path();
        let ext = path.extension().and_then(|s| s.to_str());
        if ext != Some("v") && ext != Some("sv") {
            continue;
        }

        let content = fs::read_to_string(&path).expect("Failed to read Verilog source");
        let mut removed_count = 0;
        let new_content: String = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                let should_remove = empty_modules
                    .iter()
                    .any(|module| trimmed.starts_with(&format!("{module} ")) && trimmed.ends_with("();"));

                if should_remove {
                    println!("cargo:warning=Removing empty module instantiation: {}", trimmed);
                    removed_count += 1;
                }
                !should_remove
            })
            .collect::<Vec<_>>()
            .join("\n");

        if removed_count > 0 {
            fs::write(&path, new_content).expect("Failed to write updated Verilog source");
            println!(
                "cargo:warning=Removed {} empty module instantiations from {}",
                removed_count,
                path.display()
            );
            total_removed += removed_count;
        }
    }

    if total_removed == 0 {
        println!("cargo:warning=No empty module instantiations found in Verilog sources");
    } else {
        println!(
            "cargo:warning=Removed {} empty module instantiations from Verilog sources",
            total_removed
        );
    }
}

pub fn fix_library_rpath(out_dir: &Path) {
    let lib_dir = out_dir.join("vvacDir/runtimeDir/lib/lib_arm");
    for name in ["libtbppeer.so", "libvCtb.so", "libvmri.so"] {
        let library = lib_dir.join(name);
        cmd!("patchelf", "--set-rpath", "$ORIGIN", &library)
            .run()
            .unwrap_or_else(|error| panic!("failed to set RPATH for {}: {error}", library.display()));
    }
}
