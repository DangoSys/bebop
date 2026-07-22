use crate::state;

pub fn pmctrace_ball(ball_id: u32, rob_id: u32, elapsed: u64) {
    if !state::pmctrace_enabled() {
        return;
    }

    let clk = state::rtl_clk();
    let json = format!(
        r#"{{"type":"pmctrace","clk":{},"event":"ball","ball_id":{},"rob_id":{},"elapsed":{}}}"#,
        clk, ball_id, rob_id, elapsed
    );

    state::write_trace(&json);
}

pub fn pmctrace_mem(is_store: u8, rob_id: u32, elapsed: u64) {
    if !state::pmctrace_enabled() {
        return;
    }

    let clk = state::rtl_clk();
    let event = if is_store != 0 { "store" } else { "load" };
    let json = format!(
        r#"{{"type":"pmctrace","clk":{},"event":"{}","rob_id":{},"elapsed":{}}}"#,
        clk, event, rob_id, elapsed
    );

    state::write_trace(&json);
}
