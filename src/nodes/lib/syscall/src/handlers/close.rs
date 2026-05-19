use crate::state::SyscallState;

pub fn handle_close(state: &mut SyscallState, fd: u64) -> (u64, bool) {
    if (fd as i64) != -1 && fd > 2 {
        state.open_files.remove(&fd);
    }
    (0, false)
}
