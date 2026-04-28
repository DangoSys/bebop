use crate::state::SyscallState;

pub fn handle_exit(state: &mut SyscallState, exit_code: i32) -> (u64, bool) {
    state.exit_code = Some(exit_code);
    (0, true)
}
