use std::env;
use std::path::PathBuf;

pub fn path_current_bebop_bin() -> Result<PathBuf, String> {
    env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("canonicalize current_exe: {e}"))
}

pub fn path_find_in_system_path(name: &str) -> Result<PathBuf, String> {
    let path_env = env::var("PATH").map_err(|_| "missing env PATH".to_string())?;
    env::split_paths(&path_env)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
        .ok_or(format!("{} not found in PATH", name))?
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", name))
}
