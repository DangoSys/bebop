use super::Tile;
use crate::accel::PrivateState;

impl Tile {
    pub(crate) fn mvover(&self, rs1: u64, rs2: u64) -> Result<(), String> {
        let source_core = (rs1 & 255) as usize;
        let target_core = ((rs1 >> 8) & 255) as usize;
        let source_bank = ((rs1 >> 16) & 1023) as u32;
        let target_bank = ((rs1 >> 26) & 1023) as u32;
        let source_row = (rs2 & 65535) as usize;
        let target_row = ((rs2 >> 16) & 65535) as usize;
        let rows = ((rs2 >> 32) & 65535) as usize + 1;
        let endpoints = self.private_endpoints.lock().expect("private endpoints poisoned");
        let source = endpoints.get(&source_core).ok_or_else(|| format!("core {source_core} is not a compute endpoint"))?.clone();
        let target = endpoints.get(&target_core).ok_or_else(|| format!("core {target_core} is not a compute endpoint"))?.clone();
        drop(endpoints);
        fn resolve(state: &PrivateState, bank: u32, row: usize, rows: usize) -> Result<(usize, usize, usize), String> {
            let physical = state.bank_map.resolve_group(bank, 0).ok_or_else(|| format!("private bank {bank} is not allocated"))?;
            let start = row * state.row_bytes;
            let end = start + rows * state.row_bytes;
            if end > state.banks[physical].len() { return Err("row range exceeds bank capacity".into()); }
            Ok((physical, start, end))
        }
        if source_core == target_core {
            let mut state = source.lock().expect("private banks poisoned");
            let (source, start, end) = resolve(&state, source_bank, source_row, rows)?;
            let (target, target_start, target_end) = resolve(&state, target_bank, target_row, rows)?;
            if source == target && start != target_start && start < target_end && target_start < end {
                return Err("overlapping ranges in the same bank".into());
            }
            let data = state.banks[source][start..end].to_vec();
            state.banks[target][target_start..target_end].copy_from_slice(&data);
        } else {
            // Lock endpoints in core order even for opposing simultaneous transfers.
            let (first, second) = if source_core < target_core { (&source, &target) } else { (&target, &source) };
            let mut first = first.lock().expect("private banks poisoned");
            let mut second = second.lock().expect("private banks poisoned");
            let (source, target) = if source_core < target_core { (&mut *first, &mut *second) } else { (&mut *second, &mut *first) };
            if source.row_bytes != target.row_bytes { return Err("endpoint row widths differ".into()); }
            let (source_bank, start, end) = resolve(source, source_bank, source_row, rows)?;
            let (target_bank, target_start, target_end) = resolve(target, target_bank, target_row, rows)?;
            target.banks[target_bank][target_start..target_end].copy_from_slice(&source.banks[source_bank][start..end]);
        }
        Ok(())
    }
}
