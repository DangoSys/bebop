use super::super::bank::{mem_read, BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use super::decode::{pbank, rs1_b0, rs1_iter, xs2_mem_stride};

pub fn latency(xs1: u64, _xs2: u64) -> u64 {
    rs1_iter(xs1).max(1)
}

pub fn exec(
    xs1: u64,
    xs2: u64,
    memory: &[u8],
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
    let actual_stride = if stride == 0 { 1 } else { stride };
    for i in 0..depth {
        let addr = mem_addr + i * 16 * actual_stride * line_blocks as u64;
        let bank_offset = (i as usize) * line_bytes;
        if bank_offset + line_bytes > BANK_SIZE {
            panic!(
                "mvin: bank range: bank_offset={bank_offset} line_bytes={line_bytes} depth={depth}"
            );
        }
        for j in 0..line_bytes {
            banks[p][bank_offset + j] = mem_read(memory, addr + j as u64);
        }
    }
    0
}
