pub fn handle_set_robust_list(_head: u64, len: u64) -> (u64, bool) {
    if len != 24 {
        return ((-1i64 as u64), false);
    }
    (0, false)
}
