use crate::utils::guest_range;
use bebop_memory::Memory;

pub fn handle_rt_sigprocmask(how: u64, _set: u64, oldset: u64, sigsetsize: u64, memory: &dyn Memory) -> (u64, bool) {
    if how > 2 {
        return ((-1i64 as u64), false);
    }
    if sigsetsize != 8 {
        return ((-1i64 as u64), false);
    }
    if oldset != 0 {
        let Some(offset) = guest_range(oldset, sigsetsize as usize, memory.len()) else {
            return ((-1i64 as u64), false);
        };
        memory.fill(offset, sigsetsize as usize, 0);
    }
    (0, false)
}
