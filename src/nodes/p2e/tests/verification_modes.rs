#![allow(dead_code)]
#[path = "../build/vvac.rs"]
mod vvac;

#[test]
fn requires_exact_callbacks_and_nonblocking_transport() {
    let root = std::env::temp_dir().join(format!("p2e-vvac-test-{}", std::process::id()));
    std::fs::create_dir_all(root.join("vvacDir")).unwrap();
    for mode in ["none", "access", "difftest-n"] {
        let names = match mode {
            "none" => vec!["execution_snapshot"],
            "access" => vec!["execution_snapshot", "dpi_access_write", "access_snapshot"],
            _ => vec!["execution_snapshot", "dpi_btrace", "btrace_snapshot"],
        };
        let mut db = String::from("funcDecl.db :\n");
        for name in &names {
            db += &format!("  func_name : {name}\n  is_nb : 1\n");
        }
        db += "channel.db :\nscope.db :\n  name : test.scope\nfuncCall.db :\n";
        for id in 0..names.len() {
            db += &format!("  decl_id : {id}\n  scope_id : 0\n  work_mode : 1\n");
        }
        let path = root.join("vvacDir/db.txt");
        std::fs::write(&path, &db).unwrap();
        vvac::verify_trace(&root, mode);
        assert_eq!(
            std::fs::read_to_string(root.join("p2e_execution_scopes")).unwrap(),
            "test.scope\n"
        );
        if mode != "none" {
            assert!(std::panic::catch_unwind(|| vvac::verify_trace(&root, "none")).is_err());
            std::fs::write(&path, db.replace("work_mode : 1", "work_mode : 0")).unwrap();
            assert!(std::panic::catch_unwind(|| vvac::verify_trace(&root, mode)).is_err());
        }
        std::fs::write(&path, db.replace("execution_snapshot", "missing_counter")).unwrap();
        assert!(std::panic::catch_unwind(|| vvac::verify_trace(&root, mode)).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
