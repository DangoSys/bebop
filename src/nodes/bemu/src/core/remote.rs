use crate::core::shared::State;
use crate::core::CoreState;

#[derive(Default)]
pub(crate) struct Descriptor {
    fields: [u64; 4],
    valid: u8,
}

fn range(memory: &State, base: u64, bytes: u64) -> usize {
    let bank_bytes = memory.storage.first().expect("core has no shared physical banks").len();
    let end = base.checked_add(bytes).expect("C2C physical byte range overflows");
    assert!(
        bytes != 0 && end <= (memory.storage.len() * bank_bytes) as u64,
        "C2C physical byte range exceeds shared storage"
    );
    assert!(
        bank_bytes % 16 == 0,
        "C2C bank size must contain complete 16-byte beats"
    );
    for bank in base as usize / bank_bytes..=(end as usize - 1) / bank_bytes {
        assert!(
            memory.map.slots[bank].valid,
            "C2C accesses an unallocated shared physical bank"
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

impl CoreState {
    pub(crate) fn c2c_control(&self, operation: u32, address: u64, value: u64) -> u64 {
        if (13..=15).contains(&operation) {
            assert_eq!(address >> 32, 0, "C2C control has no remote thread/context selector");
        }
        let field = address as u32 as usize;
        match operation {
            13 => {
                assert!(field < 4, "invalid C2C descriptor field");
                let mut descriptor = self.c2c.lock().expect("C2C descriptor poisoned");
                descriptor.fields[field] = value;
                descriptor.valid |= 1 << field;
            }
            14 => {
                assert_eq!(field, 0, "C2C execute reserves the field selector");
                let mut descriptor = self.c2c.lock().expect("C2C descriptor poisoned");
                assert_eq!(descriptor.valid, 15, "incomplete C2C descriptor");
                let [source, destination, target, bytes] = descriptor.fields;
                assert!(
                    source % 16 == 0 && target % 16 == 0,
                    "C2C start addresses must be 16-byte aligned"
                );
                assert!(
                    destination < crate::configs::core_count() as u64,
                    "C2C destination must be a design core ID"
                );
                assert_ne!(destination, self.hart_id as u64, "C2C requires distinct cores");
                let destination = destination as usize;
                let remote = self
                    .core_registry
                    .lock()
                    .expect("core registry poisoned")
                    .get(&destination)
                    .expect("C2C target core is not instantiated")
                    .upgrade()
                    .expect("C2C target core has been destroyed");
                // A single CoreID lock order also covers opposing simultaneous transfers.
                if self.hart_id < destination {
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
                assert!(field < 4, "invalid C2C geometry field");
                let memory = self.banks.lock().expect("shared banks poisoned");
                let bank_bytes = memory.storage.first().expect("core has no shared physical banks").len();
                return match field {
                    0 => (memory.storage.len() * bank_bytes) as u64,
                    1 => bank_bytes as u64,
                    2 => self.hart_id as u64,
                    3 => crate::configs::core_count() as u64,
                    _ => unreachable!(),
                };
            }
            16 | 17 => {
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
                assert_eq!(value, 0, "C2C lease reserves rs2");
                let owner = (address >> 32) as usize;
                assert!(
                    owner < self.endpoint_ids.len(),
                    "C2C lease endpoint is not a local Buckyball thread"
                );
                let bank = ((address >> 16) & 65535) as u32;
                let group = (address & 65535) as u32;
                // Only the shared-bank lock is needed for endpoint mapping.
                let mut memory = self.banks.lock().expect("shared banks poisoned");
                assert!(
                    (bank as usize) < memory.virtual_bank_count,
                    "C2C lease virtual bank is outside the core namespace"
                );
                let cfg = memory.cfgs[owner * memory.virtual_bank_count + bank as usize];
                assert!(
                    cfg.allocated && (group as u64) < cfg.cols,
                    "C2C lease requires a live allocated shared group"
                );
                let physical = memory
                    .map
                    .resolve_hart_group(owner, bank, group)
                    .expect("C2C lease shared group is not mapped");
                let slot = &mut memory.map.slots[physical];
                if operation == 18 {
                    slot.leases = slot.leases.checked_add(1).expect("C2C lease count overflows");
                    return (physical * memory.storage[physical].len()) as u64;
                }
                assert!(slot.leases != 0, "C2C release has no exported shared bank lease");
                slot.leases -= 1;
            }
            _ => unreachable!(),
        }
        0
    }
}
