use crate::constants::GUEST_MEM_BASE;

pub fn handle_rt_sigaction(signum: u64, _act: u64, oldact: u64, sigsetsize: u64, memory: &mut [u8]) -> (u64, bool) {
    if signum == 0 || signum > 64 {
        return ((-1i64 as u64), false);
    }
    if sigsetsize != 8 {
        return ((-1i64 as u64), false);
    }
    if oldact != 0 {
        let oldact_size = 32u64;
        if oldact < GUEST_MEM_BASE || oldact + oldact_size > GUEST_MEM_BASE + memory.len() as u64 {
            return ((-1i64 as u64), false);
        }
        let offset = (oldact - GUEST_MEM_BASE) as usize;
        memory[offset..offset + oldact_size as usize].fill(0);
    }
    (0, false)
}
