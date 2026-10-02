// Unit harness for the actual BEMU bank and instruction code; no CPU or FPGA emulation.
#![allow(dead_code, unused_imports)]
use bebop_bank_hash::access::{self, WriteRecord};
mod config {
    pub fn is_shared_vbank(v: u64) -> bool {
        v >= 32
    }
    macro_rules! constants { ($($name:ident = $value:expr),*) => { $(pub fn $name() -> usize { $value })* }; }
    constants!(
        bank_lines = 64,
        bank_num = 8,
        bank_row_bytes = 16,
        bank_size = 1024,
        bank_width = 128,
        mmio_bank_lines = 64,
        mmio_bank_num = 1,
        mmio_bank_row_bytes = 16,
        mmio_bank_size = 1024,
        mmio_bank_width = 128,
        mmio_read_width = 128,
        mmio_total_size = 1024,
        virtual_bank_num = 64
    );
    pub fn mmio_enable() -> bool {
        true
    }
}
mod bank {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../bemu/src/emu/bank/bank.rs"));
}
mod root {
    pub mod mmu {
        pub struct GuestAccess<'a>(pub &'a mut [u8]);
        impl GuestAccess<'_> {
            pub fn read(&mut self, addr: u64) -> u8 {
                self.0[addr as usize]
            }
            pub fn read_buffer(&mut self, addr: u64, data: &mut [u8]) {
                data.copy_from_slice(&self.0[addr as usize..addr as usize + data.len()]);
            }
        }
    }
}
mod trace {
    pub struct MTraceEvent {
        pub is_write: bool,
        pub is_shared: bool,
        pub channel: u32,
        pub hart_id: u64,
        pub rob_id: u32,
        pub vbank_id: u32,
        pub pbank_id: u32,
        pub group_id: u32,
        pub addr: u32,
        pub write_mask: u32,
        pub data_lo: u64,
        pub data_hi: u64,
    }
    pub fn mtrace(_: MTraceEvent) {}
}
#[path = "../../../../../../examples/balls/gemmini/emu/src/gemmini_state.rs"]
mod gemmini_state;
mod inst {
    pub(crate) use crate::{bank_matrix, gemmini_state};
    pub mod relu {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../../examples/balls/relu/emu/src/50_relu.rs"
        ));
    }
    pub mod compute {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../../examples/balls/gemmini/emu/src/66_gemmini_compute_preloaded.rs"
        ));
    }

    pub mod instruction {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../bemu/src/emu/inst/instruction.rs"
        ));
    }
    pub mod decode {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../bemu/src/emu/inst/decode.rs"
        ));
    }
    pub mod mset {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../bemu/src/emu/inst/32_mset.rs"
        ));
    }
    pub mod mvin {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../bemu/src/emu/inst/33_mvin.rs"
        ));
    }
    pub mod mvin2d {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../bemu/src/emu/inst/34_mvin_2d.rs"
        ));
    }
    pub fn cycles_after_issue(_: u32, _: u64, _: u64) -> u64 {
        unreachable!()
    }
    pub fn execute_known(_: u32, _: u64, _: u64, _: &mut instruction::ExecContext) -> Option<u64> {
        unreachable!()
    }
}
use inst::instruction;
#[path = "../../../bemu/src/emu/inst/bank_matrix.rs"]
mod bank_matrix;

#[test]
fn real_reference_writes_cover_clear_dma_compute_and_partial_rows() {
    use instruction::{ExecContext, Instruction, PrivateBank, TrackedBanks};
    let mut banks: Vec<_> = (0..8).map(|_| PrivateBank::new(1024, false)).collect();
    let mut configs = vec![bank::BankConfig::default(); 64];
    let mut map = bank::BankMap::new(8);
    let mut deferred = vec![];
    let mut mmio = vec![];
    let mut barrier = false;
    let mut memory = vec![7; 1024];
    let mut ctx = ExecContext {
        hart_id: 0,
        inst_id: 7,
        memory: root::mmu::GuestAccess(&mut memory),
        banks: TrackedBanks::new(&mut banks, None, 7),
        cfgs: &mut configs,
        bank_map: &mut map,
        shared: None,
        deferred_bank_frees: &mut deferred,
        mmio_banks: &mut mmio,
        barrier_hit: &mut barrier,
    };
    access::start();
    inst::mset::Mset::exec(2, 1 | (1 << 5) | (1 << 10) | (1 << 11), &mut ctx);
    let physical = ctx.physical_bank(2, 0);
    let mut sequence = 0;
    let mut subject = |addr, mask, data| {
        access::observe(
            true,
            WriteRecord {
                stream_hart: 0,
                hart: 0,
                inst: 7,
                shared: 0,
                bank: 2,
                group: 0,
                physical: physical as u32,
                sequence,
                addr,
                mask,
                data,
            },
        );
        sequence += 1;
    };
    for row in 0..64 {
        subject(row, 0xffff, [0; 16]);
    }
    inst::mvin::Mvin::exec((2 << 20) | (2 << 30), 1 << 39, &mut ctx);
    subject(0, 0xffff, [7; 16]);
    subject(1, 0xffff, [7; 16]);
    ctx.banks.write_row(physical, 0, [9; 16], 1);
    subject(0, 1, [9; 16]);
    assert_eq!(
        &ctx.banks[physical][..16],
        &[9, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7]
    );
    let values = vec![vec![123i32; 4]; 4];
    bank_matrix::write_i32_nn_groups(&mut ctx.banks, &[physical], &values, 4);
    let mut row = [0; 16];
    for i in 0..4 {
        row[i * 4..i * 4 + 4].copy_from_slice(&123i32.to_le_bytes());
    }
    for addr in 0..4 {
        subject(addr, 0xffff, row);
    }
    inst::mvin2d::Mvin2d::exec((2 << 20) | (1 << 30), (1 << 36) | (1 << 43) | (1 << 62), &mut ctx);
    subject(0, 0xff, [7; 16]);
    assert_eq!(&ctx.banks[physical][8..16], &row[8..16]);
    assert_eq!(access::inspect(|s| s.matched), 72);
    access::finish().unwrap();

    // Same parent InstID across DMA and compute micro-ops, including repeated row writes.
    use instruction::BallInstruction;
    access::start();
    for (vbank, groups) in [(0, 1), (1, 1), (3, 4)] {
        inst::mset::Mset::exec(vbank, 1 | (groups << 5) | (1 << 10), &mut ctx);
    }
    for vbank in 0..2 {
        inst::mvin::Mvin::exec((vbank << 20) | (16 << 30), 1 << 39, &mut ctx);
        for row in 0..16 {
            access::observe(
                true,
                WriteRecord {
                    stream_hart: 0,
                    hart: 0,
                    inst: 7,
                    shared: 0,
                    bank: vbank as u32,
                    group: 0,
                    physical: ctx.physical_bank(vbank, 0) as u32,
                    sequence: row,
                    addr: row as u32,
                    mask: 0xffff,
                    data: [7; 16],
                },
            );
        }
    }
    inst::compute::GemminiComputePreloaded::exec((1 << 10) | (3 << 20) | (16 << 30), 2, &mut ctx);
    let data = std::array::from_fn(|lane| 784i32.to_le_bytes()[lane % 4]);
    for group in 0..4 {
        for row in 0..16 {
            access::observe(
                true,
                WriteRecord {
                    stream_hart: 0,
                    hart: 0,
                    inst: 7,
                    shared: 0,
                    bank: 3,
                    group,
                    physical: ctx.physical_bank(3, group as u64) as u32,
                    sequence: row,
                    addr: row as u32,
                    mask: 0xffff,
                    data,
                },
            );
        }
    }
    inst::relu::Relu::exec(3 | (16 << 30), 64, &mut ctx);
    for row in 0..16 {
        access::observe(
            true,
            WriteRecord {
                stream_hart: 0,
                hart: 0,
                inst: 7,
                shared: 0,
                bank: 3,
                group: 0,
                physical: ctx.physical_bank(3, 0) as u32,
                sequence: 16 + row,
                addr: row as u32,
                mask: 0xffff,
                data,
            },
        );
    }
    assert_eq!(access::inspect(|s| s.matched), 112);
    access::finish().unwrap();

    let mut with_hash = PrivateBank::new(1024, true);
    with_hash[0] = 5;
    let before = with_hash.status_hash();
    let mut storage = vec![with_hash];
    TrackedBanks::new(&mut storage, None, 1).write_row(0, 0, [7; 16], 1);
    assert_ne!(storage[0].status_hash(), before);
    assert_eq!(storage[0].status_hash(), bebop_bank_hash::bank_hash(&storage[0], 16));
}
