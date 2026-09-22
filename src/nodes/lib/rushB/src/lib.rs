//! Shared Rust contract for the backend-neutral rushB host ABI.

pub const FUNCT7_FENCE: u32 = 0;
pub const FUNCT7_MVOUT: u32 = 16;
pub const FUNCT7_MSET: u32 = 32;
pub const FUNCT7_MVIN: u32 = 33;
pub const FUNCT7_MVIN_2D: u32 = 34;
pub const FUNCT7_MVIN_MMIO: u32 = 35;
pub const CORE_LOCAL_ID_BITS: u32 = 16;
pub const CORE_LOCAL_ID_MASK: u32 = (1 << CORE_LOCAL_ID_BITS) - 1;

pub const fn encode_core_id(tile_id: u32, local_id: u32) -> Option<u32> {
    if tile_id > CORE_LOCAL_ID_MASK || local_id > CORE_LOCAL_ID_MASK {
        None
    } else {
        Some((tile_id << CORE_LOCAL_ID_BITS) | local_id)
    }
}

pub const fn decode_core_id(core_id: u32) -> (u32, u32) {
    (core_id >> CORE_LOCAL_ID_BITS, core_id & CORE_LOCAL_ID_MASK)
}
