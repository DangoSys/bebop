//===- 32_mset.rs - MSET instruction (bank allocation) ---------------------===//

use super::super::bank::{is_shared_vbank, BankConfig};
use super::decode::{rs1_b0, rs1_b2, xs2_mset};
use super::instruction::{ExecContext, Instruction};

pub struct Mset;

impl Instruction for Mset {
    const FUNCT: u32 = 32;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bank_id = rs1_b0(xs1);
        let program_base = crate::config::virtual_bank_num() as u64;
        if bank_id >= program_base {
            assert!(bank_id < program_base + 2 && xs1 == bank_id && xs2 == 0,
                    "MSET program bank supports release only");
            ctx.rvv.as_mut().expect("core has no RVV IP")
                .release_program((bank_id - program_base) as usize);
            return 0;
        }
        if xs2 & (1 << 12) != 0 {
            assert_eq!(xs2, 1 << 12, "mset transfer: reserved rs2 bits must be zero");
            assert_eq!(
                xs1 & !(0x3ff | (0x3ff << 20)),
                0,
                "mset transfer: reserved rs1 bits must be zero"
            );
            let target = rs1_b2(xs1);
            assert_ne!(bank_id, target, "mset transfer: source and target must differ");
            assert_eq!(
                is_shared_vbank(bank_id),
                is_shared_vbank(target),
                "mset transfer: bank namespaces must match"
            );
            let source_cfg = *ctx.config(bank_id);
            let target_cfg = *ctx.config(target);
            assert!(source_cfg.allocated, "mset transfer: source is not allocated");
            let offset = if target_cfg.allocated { target_cfg.cols } else { 0 };
            let (map, owner) = if is_shared_vbank(bank_id) {
                let shared = ctx.shared.as_mut().expect("shared bank storage is unavailable");
                (&mut *shared.bank_map, shared.local_core)
            } else {
                (&mut *ctx.bank_map, 0)
            };
            let physical: Vec<_> = (0..source_cfg.cols)
                .map(|group| {
                    map.resolve_hart_group(owner, bank_id as u32, group as u32)
                        .expect("mset transfer: source group is not mapped")
                })
                .collect();
            assert!(physical.iter().all(|&p| map.slots[p].leases == 0),
                    "mset transfer: source has an exported shared bank lease");
            for (group, p) in physical.into_iter().enumerate() {
                map.bind_hart_group(p, owner, target as u32, offset as u32 + group as u32);
            }
            *ctx.config_mut(target) = BankConfig {
                allocated: true,
                cols: offset + source_cfg.cols,
                valid_rows: if target_cfg.allocated {
                    target_cfg.valid_rows.min(source_cfg.valid_rows)
                } else {
                    source_cfg.valid_rows
                },
            };
            *ctx.config_mut(bank_id) = BankConfig::default();
            return 0;
        }
        let (_rows, col, alloc, clear) = xs2_mset(xs2);
        assert!(!clear || alloc == 1, "mset: clear requires allocation");

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
                shared.bank_map.delete_hart_vbank(shared.local_core, v);
                for group in 0..groups {
                    let p = shared
                        .bank_map
                        .first_free_pbank()
                        .unwrap_or_else(|| panic!("mset: no free shared physical bank"));
                    shared.bank_map.bind_hart_group(p, shared.local_core, v, group as u32);
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
                    if clear {
                        ctx.banks.initialize(p, 0);
                    }
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
                shared.bank_map.delete_hart_vbank(shared.local_core, v);
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
