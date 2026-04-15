#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Spike,
    Bemu,
    Verilator,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Spike => "spike",
            NodeKind::Bemu => "bemu",
            NodeKind::Verilator => "verilator",
        }
    }
}

pub mod emu;
pub mod spike;
#[cfg(all(feature = "verilator", unix))]
pub mod verilator;
