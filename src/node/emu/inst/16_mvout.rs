use super::super::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use super::decode::{pbank, rs1_b0, rs1_iter, xs2_mem_stride};

pub fn latency(xs1: u64, _xs2: u64) -> u64 {
    rs1_iter(xs1).max(1)
}

pub fn exec(
    xs1: u64,
    xs2: u64,
    mem_write16: &mut dyn FnMut(u64, [u8; 16]),
    banks: &[Vec<u8>],
    cfgs: &[BankConfig],
    bank_map: &BankMap,
) -> u64 {
    let bank_id = rs1_b0(xs1);
    let depth = rs1_iter(xs1);
    let (mem_addr, stride) = xs2_mem_stride(xs2);
    if bank_id >= BANK_NUM as u64 {
        panic!("mvout: invalid bank_id {bank_id}");
    }
    let bi = bank_id as usize;
    if !cfgs[bi].allocated {
        panic!("mvout: bank {bank_id} not allocated");
    }
    let p = pbank(bank_map, bank_id);
    let cols = cfgs[bi].cols;
    let line_blocks = if cols == 0 { 1 } else { cols as usize };
    let line_bytes = line_blocks * 16;
    let rows = depth;
    let actual_stride = if stride == 0 { 1 } else { stride };
    for i in 0..rows {
        let bank_offset = (i as usize) * line_bytes;
        if bank_offset + line_bytes > BANK_SIZE {
            panic!(
        "mvout: bank range: bank_offset={bank_offset} line_bytes={line_bytes} rows={rows} depth={depth}"
      );
        }
        let addr_row = mem_addr + i * 16 * actual_stride * line_blocks as u64;
        for b in 0..line_blocks {
            let addr = addr_row + (b as u64) * 16;
            let off = bank_offset + b * 16;
            let mut data = [0u8; 16];
            data.copy_from_slice(&banks[p][off..off + 16]);
            mem_write16(addr, data);
        }
    }
    0
}
