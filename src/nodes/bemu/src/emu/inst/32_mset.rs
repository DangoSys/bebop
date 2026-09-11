//===- 32_mset.rs - MSET instruction (bank allocation) ---------------------===//

use super::super::bank::{is_shared_vbank, BankConfig};
use super::decode::{rs1_b0, xs2_mset};
use super::instruction::{ExecContext, Instruction};

pub struct Mset;

impl Instruction for Mset {
    const FUNCT: u32 = 32;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bank_id = rs1_b0(xs1);
        let (_rows, col, alloc) = xs2_mset(xs2);

        let v = bank_id as u32;
        let groups = col.max(1);
        let shared_bank = is_shared_vbank(bank_id);

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
                ctx.banks.initialize(p, 0);
            }
            *ctx.config_mut(bank_id) = BankConfig {
                allocated: true,
                cols: col,
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
