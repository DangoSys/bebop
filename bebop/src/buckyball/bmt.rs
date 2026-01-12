use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

struct BMTState {
  vbank_to_pbanks: HashMap<u64, Vec<u64>>,
  pbank_to_vbank: HashMap<u64, u64>,
  free_pbank_list: VecDeque<u64>,
  num_pbanks: u64,
  num_vbanks: u64,
}

static BANK_MAP_TABLE: Mutex<Option<BMTState>> = Mutex::new(None);

pub fn init_bmt(num_vbanks: u64, num_pbanks: u64) {
  let state = BMTState {
    vbank_to_pbanks: HashMap::new(),
    pbank_to_vbank: HashMap::new(),
    free_pbank_list: (0..num_pbanks).collect(),
    num_pbanks,
    num_vbanks,
  };
  *BANK_MAP_TABLE.lock().unwrap() = Some(state);
}

/// 分配物理 bank
pub fn allocate_bank(vbank_id: u64, num_pbanks: u64) -> Option<Vec<u64>> {
  let mut state_opt = BANK_MAP_TABLE.lock().unwrap();
  if let Some(ref mut state) = *state_opt {
    if state.free_pbank_list.len() < num_pbanks as usize {
      return None;
    }
    if state.vbank_to_pbanks.contains_key(&vbank_id) {
      return None;
    }

    let mut allocated = Vec::new();
    for _ in 0..num_pbanks {
      if let Some(pbank_id) = state.free_pbank_list.pop_front() {
        allocated.push(pbank_id);
      }
    }
    if !allocated.is_empty() {
      for &pbank_id in &allocated {
        state.pbank_to_vbank.insert(pbank_id, vbank_id);
      }
      state.vbank_to_pbanks.insert(vbank_id, allocated.clone());
      return Some(allocated);
    }
  }
  None
}

/// 释放虚拟 bank
pub fn free_bank(vbank_id: u64) -> bool {
  let mut state_opt = BANK_MAP_TABLE.lock().unwrap();
  if let Some(ref mut state) = *state_opt {
    if let Some(pbank_ids) = state.vbank_to_pbanks.remove(&vbank_id) {
      for pbank_id in pbank_ids {
        state.pbank_to_vbank.remove(&pbank_id);
        state.free_pbank_list.push_back(pbank_id);
      }
      return true;
    }
  }
  false
}

/// 查询虚拟 bank 对应的物理 bank 列表
pub fn get_pbank_ids(vbank_id: u64) -> Option<Vec<u64>> {
  let state_opt = BANK_MAP_TABLE.lock().unwrap();
  if let Some(ref state) = *state_opt {
    state.vbank_to_pbanks.get(&vbank_id).cloned()
  } else {
    None
  }
}

/// 查询物理 bank 被哪个虚拟 bank 占用
pub fn get_vbank_id(pbank_id: u64) -> Option<u64> {
  let state_opt = BANK_MAP_TABLE.lock().unwrap();
  if let Some(ref state) = *state_opt {
    state.pbank_to_vbank.get(&pbank_id).copied()
  } else {
    None
  }
}

pub fn print_bmt() {
  let state_opt = BANK_MAP_TABLE.lock().unwrap();
  if let Some(ref state) = *state_opt {
    println!("vbank_to_pbanks: {:?}", state.vbank_to_pbanks);
    println!("pbank_to_vbank: {:?}", state.pbank_to_vbank);
    println!("free_pbank_list: {:?}", state.free_pbank_list);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_bmt() {
    init_bmt(16, 32);
    let pbank_ids = allocate_bank(0, 1).unwrap();
    print_bmt();
    assert_eq!(pbank_ids, vec![0]);

    let pbank_ids = allocate_bank(1, 4).unwrap();
    print_bmt();
    assert!(free_bank(0));
    print_bmt();

    let pbank_ids = allocate_bank(0, 1).unwrap();
    print_bmt();
  }
}
