//! Dispatch to per-instruction `latency` in each `fXX_*` module.
//! Heuristic issue→complete cycles; not RTL-accurate.

use super::decode::{
    self, FUNCT_BFP, FUNCT_DEQUANT, FUNCT_GEMMINI_COMPUTE_ACCUMULATED, FUNCT_GEMMINI_COMPUTE_PRELOADED,
    FUNCT_GEMMINI_PRELOAD, FUNCT_IM2COL, FUNCT_MUL_WARP16, FUNCT_MVIN, FUNCT_MVOUT, FUNCT_QUANT, FUNCT_RELU,
    FUNCT_TRANSPOSE,
};
use super::{
    f00_fence, f01_barrier, f02_gemmini_config, f03_gemmini_flush, f04_bdb_counter, f16_mvout, f32_mset, f33_mvin,
    f48_im2col, f49_transpose, f50_relu, f51_quant, f52_dequant, f53_gemmini_preload, f64_mul_warp16, f65_bfp,
    f66_gemmini_compute_preloaded, f67_gemmini_compute_accumulated, f80_gemmini_loop_ws, f96_gemmini_loop_conv_ws,
};

pub fn cycles_after_issue(funct: u32, xs1: u64, xs2: u64) -> u64 {
    match funct {
        decode::FUNCT_FENCE => f00_fence::latency(xs1, xs2),
        decode::FUNCT_BARRIER => f01_barrier::latency(xs1, xs2),
        decode::FUNCT_GEMMINI_CONFIG => f02_gemmini_config::latency(xs1, xs2),
        decode::FUNCT_GEMMINI_FLUSH => f03_gemmini_flush::latency(xs1, xs2),
        decode::FUNCT_BDB_COUNTER => f04_bdb_counter::latency(xs1, xs2),
        FUNCT_MVOUT => f16_mvout::latency(xs1, xs2),
        decode::FUNCT_MSET => f32_mset::latency(xs1, xs2),
        FUNCT_MVIN => f33_mvin::latency(xs1, xs2),
        FUNCT_IM2COL => f48_im2col::latency(xs1, xs2),
        FUNCT_TRANSPOSE => f49_transpose::latency(xs1, xs2),
        FUNCT_RELU => f50_relu::latency(xs1, xs2),
        FUNCT_QUANT => f51_quant::latency(xs1, xs2),
        FUNCT_DEQUANT => f52_dequant::latency(xs1, xs2),
        FUNCT_GEMMINI_PRELOAD => f53_gemmini_preload::latency(xs1, xs2),
        FUNCT_MUL_WARP16 => f64_mul_warp16::latency(xs1, xs2),
        FUNCT_BFP => f65_bfp::latency(xs1, xs2),
        FUNCT_GEMMINI_COMPUTE_PRELOADED => f66_gemmini_compute_preloaded::latency(xs1, xs2),
        FUNCT_GEMMINI_COMPUTE_ACCUMULATED => f67_gemmini_compute_accumulated::latency(xs1, xs2),
        f if f >= decode::FUNCT_GEMMINI_LOOP_WS_CONFIG_BOUNDS && f <= decode::FUNCT_GEMMINI_LOOP_WS => {
            f80_gemmini_loop_ws::latency(funct, xs1, xs2)
        }
        f if f >= decode::FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_1 && f <= decode::FUNCT_GEMMINI_LOOP_CONV_WS => {
            f96_gemmini_loop_conv_ws::latency(funct, xs1, xs2)
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::inst::decode::FUNCT_MVIN;

    #[test]
    fn mvin_depth_is_latency() {
        let xs1 = (5u64) << 30;
        assert_eq!(cycles_after_issue(FUNCT_MVIN, xs1, 0), 5);
    }
}
