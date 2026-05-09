use super::gemmini_state::{gemini, mem_i8, mem_write_i32};

use super::decode::{
    FUNCT_GEMMINI_LOOP_CONV_WS, FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_1, FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_2,
    FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_3, FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_4, FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_5,
    FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_6, FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_7, FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_8,
    FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_9,
};

pub fn latency(funct: u32, _xs1: u64, _xs2: u64) -> u64 {
    if funct == FUNCT_GEMMINI_LOOP_CONV_WS {
        256
    } else {
        1
    }
}

pub fn exec_cfg(funct: u32, xs2: u64) -> u64 {
    let mut g = gemini().lock().unwrap();
    match funct {
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_1 => {
            g.loop_conv.batch = xs2 & 0xffff;
            g.loop_conv.in_dim = (xs2 >> 16) & 0xffff;
            g.loop_conv.in_ch = (xs2 >> 32) & 0xffff;
        }
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_2 => {
            g.loop_conv.out_ch = xs2 & 0xffff;
            g.loop_conv.out_dim = (xs2 >> 16) & 0xffff;
            g.loop_conv.stride = (xs2 >> 32) & 0xff;
            g.loop_conv.padding = (xs2 >> 40) & 0xff;
        }
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_3 => {
            g.loop_conv.kernel_dim = xs2 & 0xff;
        }
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_4 => g.loop_conv.addr_bias = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_5 => g.loop_conv.addr_input = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_6 => g.loop_conv.addr_weight = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_7 => g.loop_conv.addr_output = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_8 => {
            g.loop_conv.input_stride = xs2 & 0xffff_ffff;
            g.loop_conv.weight_stride = xs2 >> 32;
        }
        FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_9 => g.loop_conv.output_stride = xs2 & 0xffff_ffff,
        _ => panic!("gemmini_loop_conv_ws: unknown cfg funct={funct}"),
    }
    0
}

/// Degenerate conv (pointwise / 1×1 kernel): output[j] = sum_k in[k]*weight[k][j].
pub fn exec_loop(memory: &mut [u8]) -> u64 {
    let st = gemini().lock().unwrap().loop_conv.clone();
    let in_ch = st.in_ch as usize;
    let out_ch = st.out_ch as usize;
    if in_ch == 0 || out_ch == 0 {
        panic!("gemmini_loop_conv_ws: zero channels");
    }
    let in0 = st.addr_input;
    let w0 = st.addr_weight;
    let out0 = st.addr_output;

    for j in 0..out_ch {
        let mut acc = 0i32;
        for k in 0..in_ch {
            let a = mem_i8(memory, in0 + k as u64);
            let w = mem_i8(memory, w0 + (k * out_ch + j) as u64);
            acc += a as i32 * w as i32;
        }
        mem_write_i32(memory, out0 + j as u64 * 4, acc);
    }
    0
}
