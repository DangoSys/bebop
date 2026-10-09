//===- 33_mvin.rs - MVIN instruction (memory to bank) ----------------------===//

use super::decode::{pbank_group, rs1_b2, rs1_iter, xs2_mem_stride};
use super::instruction::{ExecContext, Instruction};

pub struct Mvin;

impl Instruction for Mvin {
    const FUNCT: u32 = 33;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bank_id = rs1_b2(xs1);
        let depth = rs1_iter(xs1);
        let (mem_addr, stride) = xs2_mem_stride(xs2);

        crate::config::is_shared_vbank(bank_id);

        if depth == 0 {
            panic!("mvin: depth must be > 0");
        }

        if stride == 0 {
            panic!("mvin: stride must be > 0");
        }

        if !ctx.config(bank_id).allocated {
            panic!("mvin: bank {bank_id} not allocated");
        }

        let cols = ctx.config(bank_id).cols;
        let groups = cols.max(1) as usize;
        let geometry = super::decode::DmaRows::decode(xs1, xs2, groups as u64);

        if geometry.groups > 1 {
            for row in 0..depth as usize {
                for group in 0..groups {
                    let p = pbank_group(ctx, bank_id, group as u64);
                    let bank_offset = row * 16;
                    if bank_offset + 16 > ctx.banks[p].len() {
                        panic!("mvin: bank range: bank_offset={bank_offset} line_bytes=16 depth={depth}");
                    }
                    let addr = geometry.source(row as u64, group as u64);
                    let mut bytes = [0; 16];
                    ctx.memory.read_buffer(addr, &mut bytes);
                    ctx.banks[p][bank_offset..bank_offset + 16].copy_from_slice(&bytes);
                    crate::trace::mtrace(crate::trace::MTraceEvent {
                        is_write: true,
                        is_shared: crate::config::is_shared_vbank(bank_id),
                        channel: 0,
                        hart_id: ctx.hart_id as u64,
                        rob_id: ctx.inst_id as u32,
                        vbank_id: bank_id as u32,
                        pbank_id: ctx.reported_physical_bank(bank_id, p),
                        group_id: group as u32,
                        addr: row as u32,
                        write_mask: 0xffff,
                        data_lo: u64::from_le_bytes(ctx.banks[p][bank_offset..bank_offset + 8].try_into().unwrap()),
                        data_hi: u64::from_le_bytes(
                            ctx.banks[p][bank_offset + 8..bank_offset + 16].try_into().unwrap(),
                        ),
                    });
                }
            }
        } else {
            let group = geometry.selected_group.unwrap_or(0);
            let p = pbank_group(ctx, bank_id, group);
            assert!(depth as usize <= ctx.banks[p].len() / 16, "DMA exceeds selected bank");
            let line_bytes = 16usize;
            if stride == 1 {
                ctx.memory
                    .read_buffer(mem_addr, &mut ctx.banks[p][..depth as usize * line_bytes]);
                if !crate::trace::mtrace_enabled() {
                    ctx.config_mut(bank_id).valid_rows = depth;
                    return 0;
                }
            }

            for i in 0..depth {
                let addr = geometry.source(i, 0);
                let bank_offset = (i as usize) * line_bytes;
                if bank_offset + line_bytes > ctx.banks[p].len() {
                    panic!("mvin: bank range: bank_offset={bank_offset} line_bytes={line_bytes} depth={depth}");
                }
                if stride != 1 {
                    let mut bytes = [0; 16];
                    ctx.memory.read_buffer(addr, &mut bytes);
                    ctx.banks[p][bank_offset..bank_offset + 16].copy_from_slice(&bytes);
                }
                crate::trace::mtrace(crate::trace::MTraceEvent {
                    is_write: true,
                    is_shared: crate::config::is_shared_vbank(bank_id),
                    channel: 0,
                    hart_id: ctx.hart_id as u64,
                    rob_id: ctx.inst_id as u32,
                    vbank_id: bank_id as u32,
                    pbank_id: ctx.reported_physical_bank(bank_id, p),
                    group_id: group as u32,
                    addr: i as u32,
                    write_mask: (1u32 << line_bytes) - 1,
                    data_lo: u64::from_le_bytes(ctx.banks[p][bank_offset..bank_offset + 8].try_into().unwrap()),
                    data_hi: u64::from_le_bytes(ctx.banks[p][bank_offset + 8..bank_offset + 16].try_into().unwrap()),
                });
            }
        }
        ctx.config_mut(bank_id).valid_rows = depth;
        0
    }

    fn latency(xs1: u64, _xs2: u64) -> u64 {
        rs1_iter(xs1).max(1)
    }
}
