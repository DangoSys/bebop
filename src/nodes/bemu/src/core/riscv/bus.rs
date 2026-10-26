#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Width {
    Byte = 1,
    Half = 2,
    Word = 4,
    Double = 8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BusError;

pub trait Bus {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError>;
    fn write(&mut self, address: u64, width: Width, value: u64) -> Result<(), BusError>;

    // Atomically compare and replace a physical memory word. Returns its old value.
    // The platform must serialize this with writes from every hart and DMA master.
    fn compare_exchange(&mut self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError>;
}

// A functional platform supplies a sequentially consistent view of memory.
// Reservations belong to the shared memory system, including DMA invalidation.
pub trait HartBus: Bus {
    fn load_reserved(&mut self, hart: u64, address: u64, width: Width) -> Result<u64, BusError>;
    fn store_conditional(&mut self, hart: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError>;
}
