use crate::state::SyscallState;

pub fn handle_tgkill(state: &mut SyscallState, tgid: i64, tid: i64, sig: i64) -> (u64, bool) {
    if tgid != 1 || tid != 1 || sig < 0 || sig > 64 {
        return ((-1i64 as u64), false);
    }
    if sig == 0 {
        return (0, false);
    }
    state.exit_code = Some(128 + sig as i32);
    (0, true)
}
