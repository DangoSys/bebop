use crate::constants::{ERR_FAULT, ERR_INVAL, ERR_NOENT};
use crate::utils::{guest_cstr, guest_range};

pub fn handle_readlinkat(
    _dirfd: i64,
    path_addr: u64,
    buf_addr: u64,
    buf_size: usize,
    memory: &mut [u8],
) -> (u64, bool) {
    if buf_size == 0 {
        return ((ERR_FAULT as u64), false);
    }
    let Some(buf_offset) = guest_range(buf_addr, buf_size, memory.len()) else {
        return ((ERR_FAULT as u64), false);
    };

    let Some(path_bytes) = guest_cstr(path_addr, 4096, memory) else {
        return ((ERR_FAULT as u64), false);
    };
    let path = match std::str::from_utf8(&path_bytes) {
        Ok(s) => s,
        Err(_) => return ((ERR_INVAL as u64), false),
    };

    // Special-case /proc/self/exe so the guest sees a sensible identifier
    if path == "/proc/self/exe" {
        let exe = b"/proc/self/exe";
        let n = exe.len().min(buf_size);
        memory[buf_offset..buf_offset + n].copy_from_slice(&exe[..n]);
        return (n as u64, false);
    }

    // General case: delegate to host filesystem. Linux distinguishes:
    //   - ENOENT  : path does not exist
    //   - EINVAL  : path exists but is not a symlink
    //   - >0      : symlink resolved, returns target length
    // std::filesystem::canonical() relies on this distinction; returning ENOENT for an
    // existing regular file causes it to throw filesystem_error.
    match std::fs::symlink_metadata(path) {
        Err(_) => (ERR_NOENT as u64, false),
        Ok(meta) => {
            if !meta.file_type().is_symlink() {
                return (ERR_INVAL as u64, false);
            }
            match std::fs::read_link(path) {
                Ok(target) => {
                    let target_bytes = target.to_string_lossy().as_bytes().to_vec();
                    let n = target_bytes.len().min(buf_size);
                    memory[buf_offset..buf_offset + n].copy_from_slice(&target_bytes[..n]);
                    (n as u64, false)
                }
                Err(_) => (ERR_INVAL as u64, false),
            }
        }
    }
}
