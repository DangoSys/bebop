use super::gemmini_state::gemini;

pub fn exec(xs2: u64) -> u64 {
    let mut g = gemini().lock().unwrap();
    g.cfg.dataflow = ((xs2 >> 4) & 1) as u8;
    g.cfg.a_transpose = ((xs2 >> 7) & 1) != 0;
    g.cfg.b_transpose = ((xs2 >> 8) & 1) != 0;
    0
}

pub fn latency(_xs1: u64, _xs2: u64) -> u64 {
    1
}
