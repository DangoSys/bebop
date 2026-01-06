/// ROB (Reorder Buffer) 重排序缓冲区规格说明
///
/// ## 概述
/// ROB 用于维护指令的执行顺序，确保指令按程序顺序提交。每个进入 ROB 的指令都会被分配一个唯一的 entry_id，
/// 并经历三个主要阶段：分配(Allocate) -> 调度(Dispatch) -> 提交(Commit)。
///
/// ## 条目状态 (EntryStatus)
/// 每个 ROB 条目有三种状态：
/// - **Idle**: 条目空闲状态，初始状态，可以用于分配新指令。
/// - **Allocated**: 指令已分配进入 ROB，但尚未被调度执行。表示指令已经在 ROB 缓冲区中等待。
/// - **Inflight**: 指令已被调度出去执行，正在执行单元中处理。一旦被标记为 Inflight，该条目就不会再被重复调度。
///
/// ## 状态转换流程
/// ```
/// Idle -> Allocated (rob_allocate_ext)
///            ↓
///       Inflight (rob_dispatch_int)
///            ↓
///        Idle (rob_commit_ext)
/// ```
///
/// ## 操作说明
///
/// ### 外部步骤 (External Steps)
/// 这些操作在每个周期开始时执行，由外部系统触发：
///
/// 1. **rob_allocate_ext**: 分配新条目到 ROB
///    - 找到第一个状态为 Idle 的条目
///    - 为新指令分配一个递增的 entry_id
///    - 将指令信息填入该条目，状态设置为 Allocated
///    - 返回 bool: true 表示分配成功，false 表示 ROB 已满（没有 Idle 条目），分配失败
///
/// 2. **rob_commit_ext**: 提交已完成的指令
///    - 根据提供的 rob_id 找到对应的条目
///    - 将该条目的状态重置为 Idle，使其可以再次被分配
///
/// ### 内部步骤 (Internal Step)
/// 这个操作在每个周期内部执行，用于 ROB 的内部状态更新：
///
/// 3. **rob_dispatch_int**: 调度 ROB 条目到执行单元
///    - 从索引 0 开始遍历 rob_buffer（FIFO 顺序）
///    - 找到第一个状态为 Allocated 的条目（即未被调度过的条目）
///    - 将该条目的状态标记为 Inflight，表示已被调度执行
///    - 返回该条目的信息 (entry_id, funct, xs1, xs2, domain_id)
///    - **重要**: Dispatch 不会从缓冲区中移除条目，只是标记为已调度
///    - 如果没有 Allocated 状态的条目，返回 None
///
/// ## 设计要点
///
/// 1. **顺序提交**: ROB 保证指令按照分配的顺序提交
///
/// ## 使用示例
///
/// ```rust
/// let mut rob = Rob::new(8);
/// // 初始化时所有8个entry都是Idle状态
/// // 分配指令
/// rob.rob_allocate_ext(Some((1, 10, 20, 0)));  // 找到第一个Idle entry, entry_id = 0, status -> Allocated
/// // 调度指令
/// let dispatched = rob.rob_dispatch_int();     // entry_id = 0, status -> Inflight
/// // 执行完成后提交
/// rob.rob_commit_ext(Some(0));                 // entry 0 状态 -> Idle，可以再次使用
/// ```

#[derive(PartialEq, Debug)]
enum EntryStatus {
  Allocated,
  Inflight,
  Idle,
}

pub struct RobEntry {
  funct: u32,
  xs1: u64,
  xs2: u64,
  domain_id: u8,
  status: EntryStatus,
  rob_id: u32,
}

pub struct Rob {
  rob_buffer: Vec<RobEntry>,
  allocated_id: u32,
  dispatched_inst: Option<(u32, u64, u64, u8, u32)>,
}

impl Rob {
  pub fn new(entry_num: u32) -> Self {
    let mut rob_buffer = Vec::with_capacity(entry_num as usize);
    for _ in 0..entry_num {
      rob_buffer.push(RobEntry {
        funct: 0,
        xs1: 0,
        xs2: 0,
        domain_id: 0,
        status: EntryStatus::Idle,
        rob_id: 0,
      });
    }
    Self {
      rob_buffer,
      allocated_id: 0,
      dispatched_inst: None,
    }
  }

  /// this is an external step
  /// returns true if allocation succeeded, false if ROB is full
  pub fn rob_allocate_ext(&mut self, inst_insert: Option<(u32, u64, u64, u8)>) -> bool {
    if inst_insert.is_some() {
      let (funct, xs1, xs2, domain_id) = inst_insert.unwrap();
      if let Some(entry) = find_idle_entry(&mut self.rob_buffer) {
        let rob_id = allocate_entry(&mut self.allocated_id);
        entry.rob_id = rob_id;
        entry.funct = funct;
        entry.xs1 = xs1;
        entry.xs2 = xs2;
        entry.domain_id = domain_id;
        entry.status = EntryStatus::Allocated;
        println!("ROB allocated entry: rob_id={:?}, funct={:?}, xs1={:?}, xs2={:?}, domain_id={:?}", rob_id, funct, xs1, xs2, domain_id);
        return true;
      }
      return false;
    }
    return true;
  } 

  /// this is an external step
  pub fn rob_commit_ext(&mut self, rob_id: Option<u32>) -> bool {
    if rob_id.is_some() {
      let id = rob_id.unwrap();
      commit_entry(&mut self.rob_buffer, id);
    }
    true
  }

  /// this is a internal step
  pub fn rob_dispatch_int(&mut self) -> Option<(u32, u64, u64, u8, u32)> {
    self.dispatched_inst = dispatch_entry(&mut self.rob_buffer);
    self.dispatched_inst
  }
}

/// allocate a new entry in the ROB, return the entry id
fn allocate_entry(allocated_entry: &mut u32) -> u32 {
  let rob_id = *allocated_entry;
  *allocated_entry += 1;
  rob_id
}   

/// Finds the first entry from index 0 that is Allocated and marks it as Inflight
fn dispatch_entry(rob_buffer: &mut Vec<RobEntry>) -> Option<(u32, u64, u64, u8, u32)> {
  for entry in rob_buffer.iter_mut() {
    if entry.status == EntryStatus::Allocated {
      entry.status = EntryStatus::Inflight; 
      return Some((entry.funct, entry.xs1, entry.xs2, entry.domain_id, entry.rob_id));
    }
  }
  None 
}

/// commit an entry from the ROB (set it back to Idle)
fn commit_entry(rob_buffer: &mut Vec<RobEntry>, rob_id: u32) {
  for entry in rob_buffer.iter_mut() {
    if entry.rob_id == rob_id {
      entry.status = EntryStatus::Idle;
      break;
    }
  }
}

/// find the first Idle entry in the ROB
fn find_idle_entry(rob_buffer: &mut Vec<RobEntry>) -> Option<&mut RobEntry> {
  for entry in rob_buffer.iter_mut() {
    if entry.status == EntryStatus::Idle {
      return Some(entry);
    }
  }
  None
}

#[test]
fn test_rob_new() {
  let rob = Rob::new(10);
  assert_eq!(rob.rob_buffer.len(), 10);
  for entry in rob.rob_buffer.iter() {
    assert_eq!(entry.status, EntryStatus::Idle);
  }
  assert_eq!(rob.dispatched_inst, None);
}

#[test]
fn test_rob_allocate() {
  let mut rob = Rob::new(8);
  assert!(rob.rob_allocate_ext(Some((1, 10, 20, 0))));
  let ( _, _, _, _, rob_id) = rob.rob_dispatch_int().unwrap();
  assert_eq!(rob_id, 0);
  assert!(rob.rob_allocate_ext(Some((2, 30, 40, 1))));
  let ( _, _, _, _, rob_id) = rob.rob_dispatch_int().unwrap();
  assert_eq!(rob_id, 1);
  assert!(rob.rob_allocate_ext(Some((3, 50, 60, 2))));
  let ( _, _, _, _, rob_id) = rob.rob_dispatch_int().unwrap();
  assert_eq!(rob_id, 2);
  assert!(rob.rob_allocate_ext(Some((4, 70, 80, 3))));
  let ( _, _, _, _, rob_id) = rob.rob_dispatch_int().unwrap();
  assert_eq!(rob_id, 3);
  assert!(rob.rob_allocate_ext(Some((5, 90, 100, 4))));
  let ( _, _, _, _, rob_id) = rob.rob_dispatch_int().unwrap();
  assert_eq!(rob_id, 4);
  assert_eq!(rob.rob_buffer.len(), 8);
}

#[test]
fn test_rob_full() {
  let mut rob = Rob::new(4);
  assert!(rob.rob_allocate_ext(Some((1, 10, 20, 0))));
  assert!(rob.rob_allocate_ext(Some((2, 30, 40, 1))));
  assert!(rob.rob_allocate_ext(Some((2, 30, 40, 1))));
  assert!(rob.rob_allocate_ext(Some((2, 30, 40, 1))));
  assert_eq!(rob.rob_buffer.len(), 4);

  // should be full, allocation should fail
  assert!(!rob.rob_allocate_ext(Some((2, 30, 40, 1))));
  // buffer size should remain the same
  assert_eq!(rob.rob_buffer.len(), 4);
  
  // commit one entry, should be able to allocate again
  rob.rob_commit_ext(Some(0));
  assert!(rob.rob_allocate_ext(Some((3, 50, 60, 2))));
}
