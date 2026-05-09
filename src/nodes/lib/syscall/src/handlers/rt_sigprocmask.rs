use crate::constants::GUEST_MEM_BASE;

pub fn handle_rt_sigprocmask(how: u64, _set: u64, oldset: u64, sigsetsize: u64, memory: &mut [u8]) -> (u64, bool) {
    if how > 2 {
        return ((-1i64 as u64), false);
    }
    if sigsetsize != 8 {
        return ((-1i64 as u64), false);
    }
    if oldset != 0 {
        if oldset < GUEST_MEM_BASE || oldset + sigsetsize > GUEST_MEM_BASE + memory.len() as u64 {
            return ((-1i64 as u64), false);
        }
        let offset = (oldset - GUEST_MEM_BASE) as usize;
        memory[offset..offset + sigsetsize as usize].fill(0);
    }
    (0, false)
}
