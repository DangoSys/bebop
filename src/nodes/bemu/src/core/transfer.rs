use crate::core::CoreState;
use crate::thread::execute::PrivateState;

impl CoreState {
    pub(crate) fn mvover(&self, rs1: u64, rs2: u64) -> Result<(), String> {
        let source_thread = (rs1 & 255) as usize;
        let target_thread = ((rs1 >> 8) & 255) as usize;
        let source_bank = ((rs1 >> 16) & 1023) as u32;
        let target_bank = ((rs1 >> 26) & 1023) as u32;
        let source_row = (rs2 & 65535) as usize;
        let target_row = ((rs2 >> 16) & 65535) as usize;
        let rows = ((rs2 >> 32) & 65535) as usize + 1;
        if !self
            .banks
            .lock()
            .expect("shared banks poisoned")
            .moves
            .contains(&(source_thread, target_thread))
        {
            return Err("bank movement endpoints are not connected".into());
        }
        let endpoints = self.private_endpoints.lock().expect("private endpoints poisoned");
        let source = endpoints
            .get(&source_thread)
            .ok_or_else(|| format!("thread {source_thread} is not a compute endpoint"))?
            .clone();
        let target = endpoints
            .get(&target_thread)
            .ok_or_else(|| format!("thread {target_thread} is not a compute endpoint"))?
            .clone();
        drop(endpoints);
        fn resolve(
            state: &PrivateState,
            shared: &crate::core::shared::State,
            owner: usize,
            bank: u32,
            row: usize,
            rows: usize,
        ) -> Result<(bool, usize, usize, usize), String> {
            let is_shared = bank as usize >= state.shared_vbank_base && (bank as usize) < state.bank_cfgs.len();
            if !is_shared && bank as usize > state.max_private_vbank {
                return Err(format!("invalid virtual bank {bank}"));
            }
            let (physical, capacity) = if is_shared {
                let physical = shared
                    .map
                    .resolve_hart_group(owner, bank, 0)
                    .ok_or_else(|| format!("thread {owner} shared bank {bank} is not allocated"))?;
                (physical, shared.storage[physical].len())
            } else {
                let physical = state
                    .bank_map
                    .resolve_group(bank, 0)
                    .ok_or_else(|| format!("thread {owner} private bank {bank} is not allocated"))?;
                (physical, state.banks[physical].len())
            };
            let start = row * state.row_bytes;
            let end = start + rows * state.row_bytes;
            if end > capacity {
                return Err("row range exceeds bank capacity".into());
            }
            Ok((is_shared, physical, start, end))
        }
        // Match accelerator execution's private-before-shared lock order.
        let (first, second) = if source_thread <= target_thread {
            (&source, &target)
        } else {
            (&target, &source)
        };
        let mut first = first.lock().expect("private banks poisoned");
        let mut second = (source_thread != target_thread).then(|| second.lock().expect("private banks poisoned"));
        let (source, target) = if source_thread == target_thread {
            (&*first, &*first)
        } else if source_thread < target_thread {
            (&*first, &**second.as_ref().unwrap())
        } else {
            (&**second.as_ref().unwrap(), &*first)
        };
        if source.row_bytes != target.row_bytes {
            return Err("endpoint row widths differ".into());
        }
        let mut shared = self.banks.lock().expect("shared banks poisoned");
        let (source_shared, source_bank, start, end) =
            resolve(source, &shared, source_thread, source_bank, source_row, rows)?;
        let (target_shared, target_bank, target_start, target_end) =
            resolve(target, &shared, target_thread, target_bank, target_row, rows)?;
        if source_shared == target_shared
            && (source_shared || source_thread == target_thread)
            && source_bank == target_bank
            && start != target_start
            && start < target_end
            && target_start < end
        {
            return Err("overlapping ranges in the same bank".into());
        }
        let data = if source_shared {
            shared.storage[source_bank][start..end].to_vec()
        } else {
            source.banks[source_bank][start..end].to_vec()
        };
        if target_shared {
            shared.storage[target_bank][target_start..target_end].copy_from_slice(&data);
        } else {
            let target = if target_thread <= source_thread {
                &mut *first
            } else {
                &mut **second.as_mut().unwrap()
            };
            target.banks[target_bank][target_start..target_end].copy_from_slice(&data);
        }
        Ok(())
    }
}
