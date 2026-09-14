//===- 32_mset.rs - MSET instruction (bank allocation) ---------------------===//

use super::super::bank::{is_shared_vbank, BankConfig};
use super::decode::{rs1_b0, xs2_mset};
use super::instruction::{ExecContext, Instruction};

pub struct Mset;

impl Instruction for Mset {
    const FUNCT: u32 = 32;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bank_id = rs1_b0(xs1);
        let (_rows, col, alloc, clear) = xs2_mset(xs2);

        let v = bank_id as u32;
        let shared_bank = is_shared_vbank(bank_id);
        let groups = if alloc == 1 && col == 0 {
            if shared_bank {
                ctx.shared
                    .as_ref()
                    .expect("shared bank storage is unavailable")
                    .bank_map
                    .slots
                    .len() as u64
            } else {
                crate::config::bank_num() as u64
            }
        } else {
            col.max(1)
        };

        if alloc == 1 {
            let mut allocated = Vec::with_capacity(groups as usize);
            if shared_bank {
                let shared = ctx.shared.as_mut().expect("shared bank storage is unavailable");
                shared.bank_map.delete_hart_vbank(shared.hart_id, v);
                for group in 0..groups {
                    let p = shared
                        .bank_map
                        .first_free_pbank()
                        .unwrap_or_else(|| panic!("mset: no free shared physical bank"));
                    shared.bank_map.bind_hart_group(p, shared.hart_id, v, group as u32);
                    allocated.push(ctx.banks.shared_index(p));
                }
            } else {
                ctx.bank_map.delete_vbank(v);
                for group in 0..groups {
                    let p = ctx
                        .bank_map
                        .first_free_pbank()
                        .unwrap_or_else(|| panic!("mset: no free private physical bank"));
                    ctx.bank_map.bind_group(p, v, group as u32);
                    allocated.push(p);
                }
            }
            for p in allocated {
                if shared_bank {
                    ctx.banks.initialize(p, 0);
                } else {
                    ctx.banks.allocate(p, clear);
                }
            }
            *ctx.config_mut(bank_id) = BankConfig {
                allocated: true,
                cols: groups,
                valid_rows: 0,
            };
        } else {
            if shared_bank {
                let shared = ctx.shared.as_mut().expect("shared bank storage is unavailable");
                shared.bank_map.delete_hart_vbank(shared.hart_id, v);
            } else {
                ctx.bank_map.delete_vbank(v);
            }
            *ctx.config_mut(bank_id) = BankConfig::default();
        }
        0
    }

    fn latency(_xs1: u64, _xs2: u64) -> u64 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{bank_num, bank_size, BankConfig, BankMap};
    use crate::inst::instruction::{PrivateBank, TrackedBanks};

    #[test]
    fn allocation_preserves_physical_bank_contents() {
        crate::config::configure_default();
        let mut memory = Vec::new();
        let mut storage = (0..bank_num())
            .map(|_| PrivateBank::new(bank_size()))
            .collect::<Vec<_>>();
        for bank in &mut storage {
            bank[..].fill(0x5a);
        }
        let mut configs = vec![BankConfig::default(); bank_num()];
        let mut bank_map = BankMap::new(bank_num());
        let mut deferred = Vec::new();
        let mut mmio = Vec::new();
        let mut barrier = false;
        let mut context = ExecContext {
            hart_id: 0,
            instruction_id: 0,
            memory: &mut memory,
            banks: TrackedBanks::new(&mut storage, None, 0),
            cfgs: &mut configs,
            bank_map: &mut bank_map,
            shared: None,
            deferred_bank_frees: &mut deferred,
            mmio_banks: &mut mmio,
            barrier_hit: &mut barrier,
        };

        Mset::exec(0, 0x421, &mut context);
        assert_eq!(context.bank_map.resolve(0), Some(0));
        assert_eq!(context.banks[0][0], 0x5a);

        Mset::exec(1, 0xc21, &mut context);
        assert_eq!(context.bank_map.resolve(1), Some(1));
        assert_eq!(context.banks[1][0], 0);
    }

    #[test]
    fn zero_column_allocation_uses_all_banks_and_reallocates() {
        crate::config::configure_default();
        let mut memory = Vec::new();
        let mut storage = (0..bank_num())
            .map(|_| PrivateBank::new(bank_size()))
            .collect::<Vec<_>>();
        let mut configs = vec![BankConfig::default(); bank_num()];
        let mut bank_map = BankMap::new(bank_num());
        let mut deferred = Vec::new();
        let mut mmio = Vec::new();
        let mut barrier = false;
        let mut context = ExecContext {
            hart_id: 0,
            instruction_id: 0,
            memory: &mut memory,
            banks: TrackedBanks::new(&mut storage, None, 0),
            cfgs: &mut configs,
            bank_map: &mut bank_map,
            shared: None,
            deferred_bank_frees: &mut deferred,
            mmio_banks: &mut mmio,
            barrier_hit: &mut barrier,
        };

        let alloc_all = 0x401;
        Mset::exec(0, alloc_all, &mut context);
        assert_eq!(context.config(0).cols, bank_num() as u64);
        for group in 0..bank_num() {
            assert_eq!(context.bank_map.resolve_group(0, group as u32), Some(group));
        }

        Mset::exec(0, alloc_all, &mut context);
        for group in 0..bank_num() {
            assert_eq!(context.bank_map.resolve_group(0, group as u32), Some(group));
        }
    }
}
