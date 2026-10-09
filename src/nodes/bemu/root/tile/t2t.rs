use super::Tile;
use crate::root::shared_memory::State;

#[derive(Default)]
pub(crate) struct Descriptor {
    fields: [u64; 4],
    valid: u8,
}

fn range(memory: &State, base: u64, bytes: u64) -> usize {
    let bank_bytes = memory.storage.first().expect("tile has no shared physical banks").len();
    let end = base.checked_add(bytes).expect("T2T physical byte range overflows");
    assert!(
        bytes != 0 && end <= (memory.storage.len() * bank_bytes) as u64,
        "T2T physical byte range exceeds shared storage"
    );
    assert!(
        bank_bytes % 16 == 0,
        "T2T bank size must contain complete 16-byte beats"
    );
    for bank in base as usize / bank_bytes..=(end as usize - 1) / bank_bytes {
        assert!(
            memory.map.slots[bank].valid,
            "T2T accesses an unallocated shared physical bank"
        );
    }
    bank_bytes
}

fn transfer(source: &State, target: &mut State, source_base: u64, target_base: u64, bytes: u64) {
    // Check both entire spans before modifying any destination byte.
    let source_bank_bytes = range(source, source_base, bytes);
    let target_bank_bytes = range(target, target_base, bytes);
    let mut copied = 0;
    while copied < bytes {
        let count = (bytes - copied).min(16) as usize;
        let from = (source_base + copied) as usize;
        let to = (target_base + copied) as usize;
        let mut beat = [0u8; 16];
        beat[..count].copy_from_slice(
            &source.storage[from / source_bank_bytes][from % source_bank_bytes..from % source_bank_bytes + count],
        );
        target.storage[to / target_bank_bytes][to % target_bank_bytes..to % target_bank_bytes + count]
            .copy_from_slice(&beat[..count]);
        copied += count as u64;
    }
}

impl Tile {
    pub(crate) fn t2t_control(&self, operation: u32, address: u64, value: u64) -> u64 {
        if (13..=15).contains(&operation) {
            assert_eq!(address >> 32, 0, "T2T control has no remote core/context selector");
        }
        assert!(
            self.tile_index == 0 || self.controller_id.is_some(),
            "T2T requires a tile control CPU"
        );
        let field = address as u32 as usize;
        match operation {
            13 => {
                assert!(field < 4, "invalid T2T descriptor field");
                let mut descriptor = self.t2t.lock().expect("T2T descriptor poisoned");
                descriptor.fields[field] = value;
                descriptor.valid |= 1 << field;
            }
            14 => {
                assert_eq!(field, 0, "T2T execute reserves the field selector");
                let mut descriptor = self.t2t.lock().expect("T2T descriptor poisoned");
                assert_eq!(descriptor.valid, 15, "incomplete T2T descriptor");
                let [source, destination, target, bytes] = descriptor.fields;
                assert!(
                    source % 16 == 0 && target % 16 == 0,
                    "T2T start addresses must be 16-byte aligned"
                );
                assert!(
                    destination < crate::config::tile_count() as u64,
                    "T2T destination must be a chip tile ID"
                );
                assert_ne!(destination, self.tile_index as u64, "T2T requires distinct tiles");
                let destination = destination as usize;
                let remote = self
                    .tile_registry
                    .lock()
                    .expect("tile registry poisoned")
                    .get(&destination)
                    .expect("T2T target tile is not instantiated")
                    .upgrade()
                    .expect("T2T target tile has been destroyed");
                assert!(remote.tile_index == 0 || remote.controller_id.is_some(), "T2T target has no tile control CPU");
                // A single TileID lock order also covers opposing simultaneous transfers.
                if self.tile_index < destination {
                    let source_memory = self.banks.lock().expect("shared banks poisoned");
                    let mut target_memory = remote.banks.lock().expect("shared banks poisoned");
                    transfer(&source_memory, &mut target_memory, source, target, bytes);
                } else {
                    let mut target_memory = remote.banks.lock().expect("shared banks poisoned");
                    let source_memory = self.banks.lock().expect("shared banks poisoned");
                    transfer(&source_memory, &mut target_memory, source, target, bytes);
                }
                descriptor.valid = 0;
            }
            15 => {
                assert!(field < 4, "invalid T2T geometry field");
                let memory = self.banks.lock().expect("shared banks poisoned");
                let bank_bytes = memory.storage.first().expect("tile has no shared physical banks").len();
                return match field {
                    0 => (memory.storage.len() * bank_bytes) as u64,
                    1 => bank_bytes as u64,
                    2 => self.tile_index as u64,
                    3 => crate::config::tile_count() as u64,
                    _ => unreachable!(),
                };
            }
            16 | 17 => {
                assert_eq!(self.tile_index, 0, "shared storage CPU access requires main tile");
                assert_eq!(address % 8, 0, "shared storage CPU access must be 8-byte aligned");
                let mut memory = self.banks.lock().expect("shared banks poisoned");
                let bank_bytes = range(&memory, address, 8);
                let bank = address as usize / bank_bytes;
                let offset = address as usize % bank_bytes;
                if operation == 16 {
                    return u64::from_le_bytes(memory.storage[bank][offset..offset + 8].try_into().unwrap());
                }
                memory.storage[bank][offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            }
            18 | 19 => {
                assert_eq!(value, 0, "T2T lease reserves rs2");
                let owner = (address >> 32) as usize;
                assert!(owner < self.endpoint_ids.len(),
                        "T2T lease endpoint is not a local Buckyball core");
                let bank = ((address >> 16) & 65535) as u32;
                let group = (address & 65535) as u32;
                // Only the shared-bank lock is needed for endpoint mapping.
                let mut memory = self.banks.lock().expect("shared banks poisoned");
                assert!((bank as usize) < memory.virtual_bank_count,
                        "T2T lease virtual bank is outside the tile namespace");
                let cfg = memory.cfgs[owner * memory.virtual_bank_count + bank as usize];
                assert!(cfg.allocated && (group as u64) < cfg.cols,
                        "T2T lease requires a live allocated shared group");
                let physical = memory.map.resolve_hart_group(owner, bank, group)
                    .expect("T2T lease shared group is not mapped");
                let slot = &mut memory.map.slots[physical];
                if operation == 18 {
                    slot.leases = slot.leases.checked_add(1).expect("T2T lease count overflows");
                    return (physical * memory.storage[physical].len()) as u64;
                }
                assert!(slot.leases != 0, "T2T release has no exported shared bank lease");
                slot.leases -= 1;
            }
            _ => unreachable!(),
        }
        0
    }
}
