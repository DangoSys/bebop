use super::gemmini_state::{gemini, mem_i32_le, mem_i8, mem_write_i32};

use super::decode::{
    FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_A, FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_B,
    FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_C, FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_D,
    FUNCT_GEMMINI_LOOP_WS_CONFIG_BOUNDS, FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_AB,
    FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_DC,
};

pub fn exec_cfg(funct: u32, xs2: u64) -> u64 {
    let mut g = gemini().lock().unwrap();
    match funct {
        FUNCT_GEMMINI_LOOP_WS_CONFIG_BOUNDS => {
            g.loop_ws.max_k = xs2 & 0xffff;
            g.loop_ws.max_j = (xs2 >> 16) & 0xffff;
            g.loop_ws.max_i = (xs2 >> 32) & 0xffff;
        }
        FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_A => g.loop_ws.addr_a = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_B => g.loop_ws.addr_b = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_D => g.loop_ws.addr_d = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_C => g.loop_ws.addr_c = xs2 & ((1u64 << 39) - 1),
        FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_AB => {
            g.loop_ws.stride_a = xs2 & 0xffff_ffff;
            g.loop_ws.stride_b = xs2 >> 32;
        }
        FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_DC => {
            g.loop_ws.stride_d = xs2 & 0xffff_ffff;
            g.loop_ws.stride_c = xs2 >> 32;
        }
        _ => panic!("gemmini_loop_ws: unknown cfg funct={funct}"),
    }
    0
}

/// OS CISC: mat_a_t^T * mat_b = mat_a * mat_b — use a[k][i] from memory.
pub fn exec_loop(memory: &mut [u8]) -> u64 {
    let lw = gemini().lock().unwrap().loop_ws.clone();
    let n = lw.stride_a as usize;
    if n == 0 || n > 64 {
        panic!("gemmini_loop_ws: bad stride/n");
    }
    for i in 0..n {
        for j in 0..n {
            let ii = i as u64;
            let jj = j as u64;
            let mut acc = if lw.addr_d == 0 {
                0i32
            } else {
                let off = lw.addr_d + ii * lw.stride_d + jj * 4;
                mem_i32_le(memory, off)
            };
            for k in 0..n {
                let kk = k as u64;
                let av = mem_i8(memory, lw.addr_a + kk * lw.stride_a + ii);
                let bv = mem_i8(memory, lw.addr_b + kk * lw.stride_b + jj);
                acc += av as i32 * bv as i32;
            }
            let c_off = lw.addr_c + ii * lw.stride_c + jj * 4;
            mem_write_i32(memory, c_off, acc);
        }
    }
    0
}
