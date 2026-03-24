//! ISA decode — funct7 and rs1/rs2 fields match `bb-tests/workloads/lib/bbhw/isa/isa.h`
//! (`FIELD`, `BB_BANK0`..`BB_BANK2`, `BB_ITER`).
use super::super::bank::{BankConfig, BankMap, BANK_NUM};
use super::{
    f00_fence, f01_barrier, f02_gemmini_config, f03_gemmini_flush, f04_bdb_counter, f16_mvout,
    f32_mset, f33_mvin, f48_im2col, f49_transpose, f50_relu, f51_quant, f52_dequant,
    f53_gemmini_preload, f54_bdb_backdoor, f64_mul_warp16, f65_bfp, f66_gemmini_compute_preloaded,
    f67_gemmini_compute_accumulated, f80_gemmini_loop_ws, f96_gemmini_loop_conv_ws,
};

pub const FUNCT_MVOUT: u32 = 16;
pub const FUNCT_MSET: u32 = 32;
pub const FUNCT_MVIN: u32 = 33;
pub const FUNCT_FENCE: u32 = 0;
pub const FUNCT_BARRIER: u32 = 1;
pub const FUNCT_GEMMINI_CONFIG: u32 = 2;
pub const FUNCT_GEMMINI_FLUSH: u32 = 3;
pub const FUNCT_BDB_COUNTER: u32 = 4;
pub const FUNCT_IM2COL: u32 = 48;
pub const FUNCT_TRANSPOSE: u32 = 49;
pub const FUNCT_RELU: u32 = 50;
pub const FUNCT_QUANT: u32 = 51;
pub const FUNCT_DEQUANT: u32 = 52;
pub const FUNCT_GEMMINI_PRELOAD: u32 = 53;
pub const FUNCT_BDB_BACKDOOR: u32 = 54;
pub const FUNCT_MUL_WARP16: u32 = 64;
pub const FUNCT_BFP: u32 = 65;
pub const FUNCT_GEMMINI_COMPUTE_PRELOADED: u32 = 66;
pub const FUNCT_GEMMINI_COMPUTE_ACCUMULATED: u32 = 67;
pub const FUNCT_GEMMINI_LOOP_WS_CONFIG_BOUNDS: u32 = 80;
pub const FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_A: u32 = 81;
pub const FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_B: u32 = 82;
pub const FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_D: u32 = 83;
pub const FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_C: u32 = 84;
pub const FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_AB: u32 = 85;
pub const FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_DC: u32 = 86;
pub const FUNCT_GEMMINI_LOOP_WS: u32 = 87;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_1: u32 = 96;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_2: u32 = 97;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_3: u32 = 98;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_4: u32 = 99;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_5: u32 = 100;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_6: u32 = 101;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_7: u32 = 102;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_8: u32 = 103;
pub const FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_9: u32 = 104;
pub const FUNCT_GEMMINI_LOOP_CONV_WS: u32 = 105;

#[inline]
pub fn rs1_b0(xs1: u64) -> u64 {
    xs1 & 0x3ff
}

#[inline]
pub fn rs1_b1(xs1: u64) -> u64 {
    (xs1 >> 10) & 0x3ff
}

#[inline]
pub fn rs1_b2(xs1: u64) -> u64 {
    (xs1 >> 20) & 0x3ff
}

/// `BB_ITER` — bits [63:30].
#[inline]
pub fn rs1_iter(xs1: u64) -> u64 {
    xs1 >> 30
}

#[inline]
pub fn xs2_mem_stride(xs2: u64) -> (u64, u64) {
    let mem = xs2 & ((1u64 << 39) - 1);
    let stride = (xs2 >> 39) & 0x7_ffff;
    (mem, stride)
}

#[inline]
pub fn xs2_mset(xs2: u64) -> (u64, u64, u64) {
    let row = xs2 & 0x1f;
    let col = (xs2 >> 5) & 0x1f;
    let alloc = (xs2 >> 10) & 1;
    (row, col, alloc)
}

/// 指令中的 bank 字段为 **vbank_id**；访问 `banks` 前解析为物理槽下标。
#[inline]
pub fn pbank(bm: &BankMap, vbank: u64) -> usize {
    if vbank >= BANK_NUM as u64 {
        panic!("pbank: invalid vbank_id {vbank}");
    }
    bm.resolve(vbank as u32)
        .unwrap_or_else(|| panic!("pbank: vbank {vbank} not mapped"))
}

pub fn execute_known(
    funct: u32,
    xs1: u64,
    xs2: u64,
    memory: &mut [u8],
    mem_read16: &mut dyn FnMut(u64) -> [u8; 16],
    mem_write16: &mut dyn FnMut(u64, [u8; 16]),
    banks: &mut [Vec<u8>],
    cfgs: &mut [BankConfig],
    bank_map: &mut BankMap,
) -> Option<u64> {
    let ret = match funct {
        FUNCT_FENCE => f00_fence::exec(),
        FUNCT_BARRIER => f01_barrier::exec(),
        FUNCT_GEMMINI_CONFIG => f02_gemmini_config::exec(xs2),
        FUNCT_GEMMINI_FLUSH => f03_gemmini_flush::exec(),
        FUNCT_BDB_COUNTER => f04_bdb_counter::exec(),
        FUNCT_MSET => f32_mset::exec(xs1, xs2, cfgs, banks, bank_map),
        FUNCT_MVIN => f33_mvin::exec(xs1, xs2, mem_read16, banks, cfgs, bank_map),
        FUNCT_MVOUT => f16_mvout::exec(xs1, xs2, mem_write16, banks, cfgs, bank_map),
        FUNCT_IM2COL => f48_im2col::exec(xs1, xs2, banks, cfgs, bank_map),
        FUNCT_MUL_WARP16 => f64_mul_warp16::exec(xs1, xs2, banks, cfgs, bank_map),
        FUNCT_TRANSPOSE => f49_transpose::exec(xs1, xs2, banks, cfgs, bank_map),
        FUNCT_RELU => f50_relu::exec(xs1, banks, cfgs, bank_map),
        FUNCT_QUANT => f51_quant::exec(xs1, xs2, banks, cfgs, bank_map),
        FUNCT_DEQUANT => f52_dequant::exec(xs1, xs2, banks, cfgs, bank_map),
        FUNCT_GEMMINI_PRELOAD => f53_gemmini_preload::exec(xs1, xs2, banks, cfgs, bank_map),
        FUNCT_BDB_BACKDOOR => f54_bdb_backdoor::exec(),
        FUNCT_BFP => f65_bfp::exec(xs1, xs2, banks, cfgs, bank_map),
        FUNCT_GEMMINI_COMPUTE_PRELOADED => {
            f66_gemmini_compute_preloaded::exec(xs1, xs2, banks, cfgs, bank_map)
        }
        FUNCT_GEMMINI_COMPUTE_ACCUMULATED => {
            f67_gemmini_compute_accumulated::exec(xs1, xs2, banks, cfgs, bank_map)
        }
        FUNCT_GEMMINI_LOOP_WS_CONFIG_BOUNDS
        | FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_A
        | FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_B
        | FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_D
        | FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_C
        | FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_AB
        | FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_DC => f80_gemmini_loop_ws::exec_cfg(funct, xs2),
        FUNCT_GEMMINI_LOOP_WS => f80_gemmini_loop_ws::exec_loop(memory),
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_1
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_2
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_3
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_4
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_5
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_6
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_7
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_8
        | FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_9 => f96_gemmini_loop_conv_ws::exec_cfg(funct, xs2),
        FUNCT_GEMMINI_LOOP_CONV_WS => f96_gemmini_loop_conv_ws::exec_loop(memory),
        _ => return None,
    };
    Some(ret)
}
