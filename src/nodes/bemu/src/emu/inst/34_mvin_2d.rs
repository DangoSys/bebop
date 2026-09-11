use super::super::bank::{bank_size, mem_read};
use super::decode::{pbank, rs1_b0, rs1_iter};
use super::instruction::{ExecContext, Instruction};

pub struct Mvin2d;

impl Instruction for Mvin2d {
    const FUNCT: u32 = 34;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bank_id = rs1_b0(xs1);
        let height = rs1_iter(xs1);
        let mem_addr = xs2 & 0xffff_ffff;
        let pixel_bytes = ((xs2 >> 32) & 0x7f) * 8;
        let source_width = (xs2 >> 39) & 0x3ff;
        let dst_base = (xs2 >> 49) & 0x3f;
        let width = ((xs2 >> 55) & 0x7) + 1;
        let valid_bytes = match (xs2 >> 58) & 0xf {
            0 => 16,
            value => value,
        };

        crate::config::is_shared_vbank(bank_id);
        if height == 0 || pixel_bytes == 0 || source_width == 0 || valid_bytes > 16 || valid_bytes as u64 > pixel_bytes
        {
            panic!("mvin_2d: invalid geometry");
        }
        if xs2 >> 62 != 0 {
            panic!("mvin_2d: reserved rs2 bits must be zero");
        }
        if !ctx.config(bank_id).allocated {
            panic!("mvin_2d: bank {bank_id} not allocated");
        }

        let depth = height * width;
        if dst_base + depth > bank_size() as u64 / 16 {
            panic!("mvin_2d: destination exceeds bank");
        }
        let bank = pbank(ctx, bank_id);
        let source_row_bytes = source_width * pixel_bytes;
        for y in 0..height {
            for x in 0..width {
                let row = dst_base + y * width + x;
                let source = mem_addr + y * source_row_bytes + x * pixel_bytes;
                let offset = row as usize * 16;
                for lane in 0..16 {
                    ctx.banks[bank][offset + lane] = if lane < valid_bytes as usize {
                        mem_read(ctx.memory, source + lane as u64)
                    } else {
                        0
                    };
                }
                crate::trace::mtrace(crate::trace::MTraceEvent {
                    is_write: true,
                    is_shared: crate::config::is_shared_vbank(bank_id),
                    channel: 0,
                    hart_id: ctx.hart_id as u64,
                    rob_id: ctx.instruction_id as u32,
                    vbank_id: bank_id as u32,
                    pbank_id: ctx.reported_physical_bank(bank_id, bank),
                    group_id: 0,
                    addr: row as u32,
                    write_mask: (1u32 << valid_bytes) - 1,
                    data_lo: u64::from_le_bytes(ctx.banks[bank][offset..offset + 8].try_into().unwrap()),
                    data_hi: u64::from_le_bytes(ctx.banks[bank][offset + 8..offset + 16].try_into().unwrap()),
                });
            }
        }
        let valid_rows = ctx.config(bank_id).valid_rows;
        ctx.config_mut(bank_id).valid_rows = (dst_base + depth).max(valid_rows);
        0
    }

    fn latency(xs1: u64, xs2: u64) -> u64 {
        rs1_iter(xs1) * (((xs2 >> 55) & 0x7) + 1)
    }
}
