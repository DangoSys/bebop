use super::decode::{pbank, rs1_iter};
use super::instruction::{ExecContext, Instruction};

pub struct Mvin2d;

impl Instruction for Mvin2d {
    const FUNCT: u32 = 34;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let geometry = super::decode::Mvin2dGeometry::decode(xs1, xs2);
        let bank_id = geometry.bank;
        let height = geometry.height;
        let width = geometry.width;
        let depth = height * width;
        let dst_base = geometry.dst_base;
        let valid_bytes = geometry.valid_bytes;
        crate::config::is_shared_vbank(bank_id);
        if !ctx.config(bank_id).allocated {
            panic!("mvin_2d: bank {bank_id} not allocated");
        }

        let bank = pbank(ctx, bank_id);
        for y in 0..height {
            for x in 0..width {
                let row = dst_base + y * width + x;
                let source = geometry.source(y, x);
                let offset = row as usize * 16;
                let valid = valid_bytes as usize;
                ctx.memory.read_buffer(source, &mut ctx.banks[bank][offset..offset + valid]);
                ctx.banks[bank][offset + valid..offset + 16].fill(0);
                crate::trace::mtrace(crate::trace::MTraceEvent {
                    is_write: true,
                    is_shared: crate::config::is_shared_vbank(bank_id),
                    channel: 0,
                    hart_id: ctx.hart_id as u64,
                    rob_id: ctx.inst_id as u32,
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
        rs1_iter(xs1) * (((xs2 >> 59) & 0x7) + 1)
    }
}
