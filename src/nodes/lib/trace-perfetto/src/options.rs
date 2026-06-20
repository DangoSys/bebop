#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub tick_ns: u64,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self { tick_ns: 1 }
    }
}
