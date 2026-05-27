use crate::utils::guest_range;

pub fn handle_rt_sigaction(signum: u64, _act: u64, oldact: u64, sigsetsize: u64, memory: &mut [u8]) -> (u64, bool) {
    if signum == 0 || signum > 64 {
        return ((-1i64 as u64), false);
    }
    if sigsetsize != 8 {
        return ((-1i64 as u64), false);
    }
    if oldact != 0 {
        let oldact_size = 32usize;
        let Some(offset) = guest_range(oldact, oldact_size, memory.len()) else {
            return ((-1i64 as u64), false);
        };
        memory[offset..offset + oldact_size].fill(0);
    }
    (0, false)
}
