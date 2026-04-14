use super::super::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use super::decode::{pbank, rs1_b0, rs1_iter, xs2_mem_stride};

pub fn latency(xs1: u64, _xs2: u64) -> u64 {
    rs1_iter(xs1).max(1)
}

pub fn exec(
    xs1: u64,
    xs2: u64,
    mem_read16: &mut dyn FnMut(u64) -> [u8; 16],
    banks: &mut [Vec<u8>],
    cfgs: &[BankConfig],
    bank_map: &BankMap,
) -> u64 {
    let bank_id = rs1_b0(xs1);
    let depth = rs1_iter(xs1);
    let (mem_addr, stride) = xs2_mem_stride(xs2);
    if bank_id >= BANK_NUM as u64 {
        panic!("mvin: invalid bank_id {bank_id}");
    }
    let bi = bank_id as usize;
    if !cfgs[bi].allocated {
        panic!("mvin: bank {bank_id} not allocated");
    }
    let p = pbank(bank_map, bank_id);
    let cols = cfgs[bi].cols;
    let line_blocks = if cols == 0 { 1 } else { cols as usize };
    let line_bytes = line_blocks * 16;
    let rows = depth;
    let actual_stride = if stride == 0 { 1 } else { stride };
    for i in 0..rows {
        let addr_row = mem_addr + i * 16 * actual_stride * line_blocks as u64;
        let bank_offset = (i as usize) * line_bytes;
        if bank_offset + line_bytes > BANK_SIZE {
            panic!(
                "mvin: bank range: bank_offset={bank_offset} line_bytes={line_bytes} rows={rows} depth={depth}"
            );
        }
        for b in 0..line_blocks {
            let addr = addr_row + (b as u64) * 16;
            let data = mem_read16(addr);
            let off = bank_offset + b * 16;
            banks[p][off..off + 16].copy_from_slice(&data);
        }
    }
    0
}
