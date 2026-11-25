use super::context::{ExecContext, Instruction};
use super::decode::{pbank_group, rs1_b0, rs1_b1, rs1_b2, rs1_iter};

pub struct Geometry {
    pub bank: u64,
    pub group: u64,
    pub first: u64,
    pub bank_stride: u64,
    pub rows: u64,
    pub address: u64,
    pub memory_stride: u64,
    pub bytes: usize,
    pub block_log2: u32,
}

impl Geometry {
    /// `blocks`: MVIN_BLOCKS / MVOUT_BLOCKS, whole 16-byte rows with the DDR
    /// stride between blocks in rows, rs2[54:39], instead of the byte stride
    /// rs2[50:39] and the valid bytes rs2[54:51].
    pub fn decode(xs1: u64, xs2: u64, write: bool, blocks: bool) -> Self {
        assert_eq!(xs2 >> 63, 0, "strided DMA: reserved rs2 bit");
        assert_eq!(xs1 & (1 << 29), 0, "strided DMA: reserved rs1 bit");
        assert_eq!(xs1 >> 63, u64::from(!write), "strided DMA: read must update its bank");
        let value = Self {
            bank: if write { rs1_b0(xs1) } else { rs1_b2(xs1) },
            group: (xs2 >> 55) & 31,
            first: if write { rs1_b1(xs1) } else { rs1_b0(xs1) },
            bank_stride: if write { rs1_b2(xs1) + 1 } else { rs1_b1(xs1) + 1 },
            rows: rs1_iter(xs1),
            address: xs2 & ((1 << 39) - 1),
            memory_stride: if blocks {
                ((xs2 >> 39) & 0xffff) * 16
            } else {
                (xs2 >> 39) & 4095
            },
            bytes: if blocks { 16 } else { (((xs2 >> 51) & 15) + 1) as usize },
            block_log2: ((xs2 >> 60) & 7) as u32,
        };
        assert!(
            value.rows > 0 && value.memory_stride > 0,
            "strided DMA: empty span or zero stride"
        );
        assert!(value.block_log2 <= 4, "strided DMA: block exponent exceeds four");
        let block = 1u64 << value.block_log2;
        assert_eq!(value.rows & (block - 1), 0, "strided DMA: incomplete block");
        assert!(
            value.block_log2 == 0
                || (value.bytes == 16 && value.bank_stride >= block && value.memory_stride >= block * 16),
            "strided DMA: blocks must contain full rows and disjoint spans"
        );
        value
    }
}

fn transfer(xs1: u64, xs2: u64, ctx: &mut ExecContext, write: bool, blocks: bool) -> u64 {
    let g = Geometry::decode(xs1, xs2, write, blocks);
    assert!(
        !generic::configs::is_shared_vbank(g.bank),
        "strided DMA: DDR cannot access shared banks"
    );
    let config = *ctx.config(g.bank);
    assert!(
        config.allocated && g.group < config.cols,
        "strided DMA: group is not allocated"
    );
    let bank = pbank_group(ctx, g.bank, g.group);
    let mask = (1u64 << g.block_log2) - 1;
    let last = g.first + ((g.rows - 1) >> g.block_log2) * g.bank_stride + mask;
    assert!(
        last < ctx.banks[bank].len() as u64 / 16,
        "strided DMA: bank span exceeded"
    );
    for i in 0..g.rows {
        let address = g.address + (i >> g.block_log2) * g.memory_stride + (i & mask) * 16;
        let row = g.first + (i >> g.block_log2) * g.bank_stride + (i & mask);
        let offset = row as usize * 16;
        let mut data = [0u8; 16];
        if write {
            data.copy_from_slice(&ctx.banks[bank][offset..offset + 16]);
            ctx.memory.write_buffer(address, &data[..g.bytes]);
        } else {
            ctx.memory.read_buffer(address, &mut data[..g.bytes]);
            ctx.banks[bank][offset..offset + 16].copy_from_slice(&data);
        }
        generic::trace::mtrace(generic::trace::MTraceEvent {
            is_write: !write,
            is_shared: false,
            channel: 0,
            hart_id: ctx.hart_id as u64,
            rob_id: ctx.inst_id as u32,
            vbank_id: g.bank as u32,
            pbank_id: ctx.reported_physical_bank(g.bank, bank),
            group_id: g.group as u32,
            addr: row as u32,
            write_mask: if write { 0 } else { 0xffff },
            data_lo: u64::from_le_bytes(data[..8].try_into().unwrap()),
            data_hi: u64::from_le_bytes(data[8..].try_into().unwrap()),
        });
    }
    if !write {
        let valid = ctx.config(g.bank).valid_rows;
        ctx.config_mut(g.bank).valid_rows = valid.max(last + 1);
    }
    0
}

pub struct MvinStrided;
impl Instruction for MvinStrided {
    const FUNCT: u32 = 36;
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        transfer(xs1, xs2, ctx, false, false)
    }
    fn latency(xs1: u64, xs2: u64, ctx: &ExecContext) -> u64 {
        let g = Geometry::decode(xs1, xs2, false, false);
        ctx.memory.transfer_cycles(ctx.cycle, g.rows, false)
    }
}

pub struct MvoutStrided;
impl Instruction for MvoutStrided {
    const FUNCT: u32 = 18;
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        transfer(xs1, xs2, ctx, true, false)
    }
    fn latency(xs1: u64, xs2: u64, ctx: &ExecContext) -> u64 {
        let g = Geometry::decode(xs1, xs2, true, false);
        ctx.memory.transfer_cycles(ctx.cycle, g.rows, true)
    }
}

pub struct MvinBlocks;
impl Instruction for MvinBlocks {
    const FUNCT: u32 = 37;
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        transfer(xs1, xs2, ctx, false, true)
    }
    fn latency(xs1: u64, xs2: u64, ctx: &ExecContext) -> u64 {
        let g = Geometry::decode(xs1, xs2, false, true);
        ctx.memory.transfer_cycles(ctx.cycle, g.rows, false)
    }
}

pub struct MvoutBlocks;
impl Instruction for MvoutBlocks {
    const FUNCT: u32 = 19;
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        transfer(xs1, xs2, ctx, true, true)
    }
    fn latency(xs1: u64, xs2: u64, ctx: &ExecContext) -> u64 {
        let g = Geometry::decode(xs1, xs2, true, true);
        ctx.memory.transfer_cycles(ctx.cycle, g.rows, true)
    }
}
