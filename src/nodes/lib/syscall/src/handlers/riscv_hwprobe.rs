use crate::utils::guest_range;

pub fn handle_riscv_hwprobe(
    pairs_addr: u64,
    pair_count: usize,
    _cpu_count: u64,
    _cpus: u64,
    flags: u64,
    memory: &mut [u8],
) -> (u64, bool) {
    if flags != 0 {
        return ((-1i64 as u64), false);
    }
    let pair_size = 16usize;
    if pair_count > 0 {
        let total_size = match pair_count.checked_mul(pair_size) {
            Some(v) => v as u64,
            None => return ((-1i64 as u64), false),
        };
        if guest_range(pairs_addr, total_size as usize, memory.len()).is_none() {
            return ((-1i64 as u64), false);
        }

        for i in 0..pair_count {
            let item_addr = pairs_addr + (i * pair_size) as u64;
            let item_offset = guest_range(item_addr, pair_size, memory.len()).unwrap();
            let mut key_bytes = [0u8; 8];
            key_bytes.copy_from_slice(&memory[item_offset..item_offset + 8]);
            let key = i64::from_le_bytes(key_bytes);

            let (new_key, value): (i64, u64) = match key {
                0 => (0, 0),
                1 => (1, 0),
                2 => (2, 0),
                3 => (3, 1),
                4 => (4, (1 << 0) | (1 << 1)),
                5 => (5, 0),
                _ => (-1, 0),
            };

            memory[item_offset..item_offset + 8].copy_from_slice(&new_key.to_le_bytes());
            memory[item_offset + 8..item_offset + 16].copy_from_slice(&value.to_le_bytes());
        }
    }
    (0, false)
}
