use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};

const LOCK_SUFFIX: &str = ".lock";
static NODE_ID: OnceLock<u32> = OnceLock::new();
static NODE_FILE: OnceLock<String> = OnceLock::new();

#[derive(Serialize, Deserialize, Default)]
struct NodeState {
    next_id: u32,
    child_pids: Vec<i32>,
}

pub fn set_node_id(id: u32) -> Result<(), String> {
    NODE_ID
        .set(id)
        .map_err(|_| "node id already initialized".to_string())
}

pub fn node_id() -> u32 {
    *NODE_ID.get().expect("node id is not initialized")
}

pub fn is_node0() -> bool {
    node_id() == 0
}

pub fn init_node(node_file: Option<&str>) -> Result<(), String> {
    match node_file {
        Some(f) => {
            NODE_FILE
                .set(f.to_string())
                .map_err(|_| "node file already initialized".to_string())?;
            let id = alloc_node_id(f)?;
            set_node_id(id)
        }
        None => {
            let f = node0_init()?;
            NODE_FILE
                .set(f)
                .map_err(|_| "node file already initialized".to_string())?;
            set_node_id(0)
        }
    }
}

pub fn node_file() -> Result<String, String> {
    NODE_FILE
        .get()
        .cloned()
        .ok_or("node file is not initialized".to_string())
}

fn lock_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(LOCK_SUFFIX);
    PathBuf::from(s)
}

fn acquire_lock(p: &Path) -> Result<PathBuf, String> {
    let lock = lock_path(p);
    if let Some(dir) = lock.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create node dir: {e}"))?;
    }
    loop {
        let f = OpenOptions::new().create_new(true).write(true).open(&lock);
        match f {
            Ok(_) => return Ok(lock),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(e) => return Err(format!("acquire node lock: {e}")),
        }
    }
}

fn read_state(p: &Path) -> Result<NodeState, String> {
    let mut s = String::new();
    OpenOptions::new()
        .read(true)
        .open(p)
        .and_then(|mut f| f.read_to_string(&mut s))
        .map_err(|e| format!("read node file: {e}"))?;
    toml::from_str::<NodeState>(&s).map_err(|e| format!("parse node file: {e}"))
}

fn write_state(p: &Path, st: &NodeState) -> Result<(), String> {
    let s = toml::to_string(st).map_err(|e| format!("encode node file: {e}"))?;
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(p)
        .and_then(|mut f| f.write_all(s.as_bytes()))
        .map_err(|e| format!("write node file: {e}"))
}

pub fn node0_init() -> Result<String, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("get time: {e}"))?
        .as_millis();
    let p = PathBuf::from(format!("/tmp/{}_bebop_node", ts));
    let lock = acquire_lock(&p)?;
    let res = (|| -> Result<String, String> {
        let st = NodeState {
            next_id: 1,
            child_pids: Vec::new(),
        };
        write_state(&p, &st)?;
        p.into_os_string()
            .into_string()
            .map_err(|_| "node file path is not valid UTF-8".to_string())
    })();
    let _ = fs::remove_file(lock);
    res
}

pub fn alloc_node_id(node_file: &str) -> Result<u32, String> {
    let p = Path::new(node_file);
    let lock = acquire_lock(p)?;
    let res = (|| -> Result<u32, String> {
        if !p.is_file() {
            return Err("node file not found".to_string());
        }
        let mut st = read_state(p)?;
        if st.next_id == 0 {
            return Err("node next_id must be > 0".to_string());
        }
        let id = st.next_id;
        st.next_id = st.next_id.checked_add(1).ok_or("node id overflow")?;
        write_state(p, &st)?;
        Ok(id)
    })();
    let _ = fs::remove_file(lock);
    res
}

pub fn add_child_pid(pid: i32) -> Result<(), String> {
    if !is_node0() {
        return Ok(());
    }
    let file = node_file()?;
    let p = Path::new(&file);
    let lock = acquire_lock(p)?;
    let res = (|| -> Result<(), String> {
        let mut st = read_state(p)?;
        if !st.child_pids.contains(&pid) {
            st.child_pids.push(pid);
        }
        write_state(p, &st)
    })();
    let _ = fs::remove_file(lock);
    res
}

pub fn remove_child_pid(pid: i32) -> Result<(), String> {
    if !is_node0() {
        return Ok(());
    }
    let file = node_file()?;
    let p = Path::new(&file);
    let lock = acquire_lock(p)?;
    let res = (|| -> Result<(), String> {
        let mut st = read_state(p)?;
        st.child_pids.retain(|v| *v != pid);
        write_state(p, &st)
    })();
    let _ = fs::remove_file(lock);
    res
}

pub fn kill_all_children() -> Result<(), String> {
    if !is_node0() {
        return Ok(());
    }
    let file = node_file()?;
    let p = Path::new(&file);
    let lock = acquire_lock(p)?;
    let pids = (|| -> Result<Vec<i32>, String> {
        let mut st = read_state(p)?;
        let out = st.child_pids.clone();
        st.child_pids.clear();
        write_state(p, &st)?;
        Ok(out)
    })()?;
    let _ = fs::remove_file(lock);

    for pid in &pids {
        let _ = kill(Pid::from_raw(*pid), Signal::SIGTERM);
    }
    thread::sleep(Duration::from_millis(200));
    for pid in &pids {
        let _ = kill(Pid::from_raw(*pid), Signal::SIGKILL);
    }
    Ok(())
}
