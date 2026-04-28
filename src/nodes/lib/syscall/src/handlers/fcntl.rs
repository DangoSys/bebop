pub fn handle_fcntl(_fd: i64, cmd: i32) -> (u64, bool) {
    match cmd {
        1 => (0, false),
        3 => (0, false),
        _ => (0, false),
    }
}
