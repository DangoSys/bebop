use crate::buckyball::lib::operation::{ExternalOp, InternalOp};
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
/// ```text
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
/// use bebop::buckyball::Rob;
/// let mut rob = Rob::new(8);
/// // 初始化时所有8个entry都是Idle状态
/// // 分配指令
/// rob.rob_allocate_ext(Some((1, 10, 20, 0)));  // 找到第一个Idle entry, rob_id = 0, status -> Allocated
/// // 调度指令
/// let dispatched = rob.rob_dispatch_int();     // rob_id = 0, status -> Inflight
/// // 执行完成后提交
/// rob.rob_commit_ext(Some(0));                 // entry 0 状态 -> Idle，可以再次使用
/// ```
use crate::simulator::simulator::FENCE_CSR;
use std::sync::atomic::Ordering;

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
  domain_id: u32,
  status: EntryStatus,
  rob_id: u32,
}

pub struct Rob {
  rob_buffer: Vec<RobEntry>,
  allocated_id: u32,
  dispatched_inst: Option<(u32, u64, u64, u32, u32)>,
  inst_insert: Option<(u32, u64, u64, u32)>,
  commit_rob_id: Option<u32>,
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
      inst_insert: None,
      commit_rob_id: None,
    }
  }

  // Operations
  pub fn allocate(&mut self) -> RobAllocate {
    RobAllocate(self)
  }
  pub fn commit(&mut self) -> RobCommit {
    RobCommit(self)
  }
  pub fn dispatch(&mut self) -> RobDispatch {
    RobDispatch(self)
  }

  // Helper Functions
  pub fn is_rob_full(&self) -> bool {
    self.rob_buffer.iter().all(|entry| entry.status != EntryStatus::Idle)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
// --- External: Allocate ---
pub struct RobAllocate<'a>(&'a mut Rob);
impl<'a> ExternalOp for RobAllocate<'a> {
  type Input = Option<(u32, u64, u64, u32)>;

  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && self.0.rob_buffer.iter().any(|e| e.status == EntryStatus::Idle)
  }

  fn has_input(&self, _input: &Self::Input) -> bool {
    _input.is_some()
  }

  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    let (funct, xs1, xs2, domain_id) = input.unwrap();
    if let Some(entry) = find_idle_entry(&mut self.0.rob_buffer) {
      let rob_id = allocate_entry(&mut self.0.allocated_id);
      entry.rob_id = rob_id;
      entry.funct = funct;
      entry.xs1 = xs1;
      entry.xs2 = xs2;
      entry.domain_id = domain_id;
      entry.status = EntryStatus::Allocated;
      println!("[Rob] Allocated instruction: funct={:?}", funct);
    }
  }
}

// --- External: Commit ---
pub struct RobCommit<'a>(&'a mut Rob);
impl<'a> ExternalOp for RobCommit<'a> {
  type Input = Option<u32>;

  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && true
  }

  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }

  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    let id = input.unwrap();
    commit_entry(&mut self.0.rob_buffer, id);
  }
}

// --- Internal: Dispatch ---
pub struct RobDispatch<'a>(&'a mut Rob);
impl<'a> InternalOp for RobDispatch<'a> {
  type Output = Option<(u32, u64, u64, u32, u32)>;

  fn has_output(&self) -> bool {
    self.0.dispatched_inst.is_some()
  }

  fn update(&mut self) {
    // for entry in self.0.rob_buffer.iter_mut() {
    //   println!("RobEntry: {:?}, {:?}", entry.status, entry.funct);
    // }
    if is_empty(&self.0.rob_buffer) {
      FENCE_CSR.store(false, Ordering::Relaxed);
    }
    self.0.dispatched_inst = dispatch_entry(&mut self.0.rob_buffer);
  }

  fn output(&mut self) -> Self::Output {
    if !self.has_output() {
      return None;
    }
    let result = self.0.dispatched_inst;
    self.0.dispatched_inst = None;
    println!("[Rob] Dispatched instruction: {:?}", result.unwrap().0);
    return result;
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
/// allocate a new entry in the ROB, return the entry id
fn allocate_entry(allocated_entry: &mut u32) -> u32 {
  let rob_id = *allocated_entry;
  *allocated_entry += 1;
  rob_id
}

/// Finds the first entry from index 0 that is Allocated and marks it as Inflight
fn dispatch_entry(rob_buffer: &mut Vec<RobEntry>) -> Option<(u32, u64, u64, u32, u32)> {
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

/// check if ROB is empty (all entries are Idle)
fn is_empty(rob_buffer: &Vec<RobEntry>) -> bool {
  rob_buffer.iter().all(|entry| entry.status == EntryStatus::Idle)
}

/// ------------------------------------------------------------
/// --- Test Functions ---
/// ------------------------------------------------------------
#[test]
fn test_rob_new() {
  let rob = Rob::new(10);
  assert_eq!(rob.rob_buffer.len(), 10);
  for entry in rob.rob_buffer.iter() {
    assert_eq!(entry.status, EntryStatus::Idle);
  }
  assert_eq!(rob.dispatched_inst, None);
}
