use crate::simulator::server::socket::{DmaReadHandler, DmaWriteHandler};
use std::sync::{Arc, Mutex};

// Gemmini parameters (from gemmini_params.h)
pub const DIM: usize = 16;
pub const ADDR_LEN: usize = 32;
pub const BANK_NUM: usize = 4;
pub const BANK_ROWS: usize = 4096;
pub const ACC_ROWS: usize = 1024;
pub const MAX_BYTES: usize = 64;
pub const MAX_BLOCK_LEN: usize = MAX_BYTES / DIM;
pub const MAX_BLOCK_LEN_ACC: usize = MAX_BYTES / (DIM * 4);
pub const LOAD_STATES: usize = 3;
pub const NORM_STAT_IDS: usize = 4;
pub const NUM_COUNTERS: usize = 8;
pub const NUM_EXTERNAL_COUNTERS: usize = 6;

pub const SP_MATRICES: usize = (BANK_NUM * BANK_ROWS) / DIM;
pub const ACCUM_ROWS: usize = ACC_ROWS;

// Type aliases
pub type ElemT = i8;
pub type AccT = i32;
pub type FullT = i64;
pub type ScaleT = f32;
pub type AccScaleT = f32;
pub type OutputT = AccT;
pub type RegT = u64;

const ELEM_T_MAX: ElemT = i8::MAX;
const ELEM_T_MIN: ElemT = i8::MIN;
const MVIN_SCALE_IDENTITY: ScaleT = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dataflow {
  OS, // Output Stationary
  WS, // Weight Stationary
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
  NONE,
  RELU,
  LAYERNORM,
  IGELU,
  SOFTMAX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormCmd {
  RESET,
  SUM,
  MEAN,
  VARIANCE,
  INV_STDDEV,
  MAX,
  SUM_EXP,
  INV_SUM_EXP,
}

pub struct GemminiState {
  pub enable: bool,
  pub resetted: bool,

  // Address and configuration
  pub output_sp_addr: u32,
  pub preload_sp_addr: u32,
  pub preload_cols: u16,
  pub preload_rows: u16,
  pub output_cols: u16,
  pub output_rows: u16,

  // Dataflow and activation
  pub mode: Dataflow,
  pub sys_act: Activation,
  pub acc_act: Activation,
  pub sys_shift: RegT,
  pub sys_acc_shift: RegT,

  // Load/store configuration
  pub load_strides: [RegT; LOAD_STATES],
  pub store_stride: RegT,
  pub load_block_strides: [u16; LOAD_STATES],
  pub load_shrunks: [bool; LOAD_STATES],
  pub load_scales: [ScaleT; LOAD_STATES],
  pub pixels_per_rows: [u8; LOAD_STATES],
  pub acc_shift: AccScaleT,
  pub c_stride: u16,
  pub a_stride: u16,

  // Pooling configuration
  pub pool_stride: u8,
  pub pool_size: u8,
  pub pool_out_dim: u8,
  pub pool_porows: u8,
  pub pool_pocols: u8,
  pub pool_orows: u8,
  pub pool_ocols: u8,
  pub pool_lpad: u8,
  pub pool_upad: u8,

  // Transpose flags
  pub a_transpose: bool,
  pub b_transpose: bool,

  // Loop WS configuration
  pub loop_ws_I: u16,
  pub loop_ws_J: u16,
  pub loop_ws_K: u16,
  pub loop_ws_pad_I: u16,
  pub loop_ws_pad_J: u16,
  pub loop_ws_pad_K: u16,
  pub loop_ws_A: u64,
  pub loop_ws_B: u64,
  pub loop_ws_D: u64,
  pub loop_ws_C: u64,
  pub loop_ws_A_stride: u64,
  pub loop_ws_B_stride: u64,
  pub loop_ws_D_stride: u64,
  pub loop_ws_C_stride: u64,

  // Loop Conv WS configuration
  pub loop_conv_ws_batch_size: u16,
  pub loop_conv_ws_in_row_dim: u16,
  pub loop_conv_ws_in_col_dim: u16,
  pub loop_conv_ws_in_channels: u16,
  pub loop_conv_ws_out_channels: u16,
  pub loop_conv_ws_in_stride: u16,
  pub loop_conv_ws_weight_stride: u16,
  pub loop_conv_ws_out_stride: u16,
  pub loop_conv_ws_out_row_dim: u16,
  pub loop_conv_ws_pool_out_row_dim: u16,
  pub loop_conv_ws_out_col_dim: u16,
  pub loop_conv_ws_pool_out_col_dim: u16,
  pub loop_conv_ws_stride: u16,
  pub loop_conv_ws_padding: u16,
  pub loop_conv_ws_kernel_dim: u16,
  pub loop_conv_ws_pool_size: u16,
  pub loop_conv_ws_pool_stride: u16,
  pub loop_conv_ws_pool_padding: u16,
  pub loop_conv_ws_batches: u16,
  pub loop_conv_ws_porows: u16,
  pub loop_conv_ws_pocols: u16,
  pub loop_conv_ws_pochs: u16,
  pub loop_conv_ws_krows: u16,
  pub loop_conv_ws_kcols: u16,
  pub loop_conv_ws_kchs: u16,
  pub loop_conv_ws_lpad: u16,
  pub loop_conv_ws_rpad: u16,
  pub loop_conv_ws_upad: u16,
  pub loop_conv_ws_dpad: u16,
  pub loop_conv_ws_plpad: u16,
  pub loop_conv_ws_prad: u16,
  pub loop_conv_ws_pupad: u16,
  pub loop_conv_ws_pdpad: u16,
  pub loop_conv_ws_orows: u16,
  pub loop_conv_ws_ocols: u16,
  pub loop_conv_ws_kernel_dilation: u16,
  pub loop_conv_ws_input: u64,
  pub loop_conv_ws_weights: u64,
  pub loop_conv_ws_output: u64,
  pub loop_conv_ws_bias: u64,

  // Normalization parameters
  pub igelu_qb: AccT,
  pub igelu_qc: AccT,
  pub qln2: AccT,
  pub qln2_inv: AccT,
  pub norm_stat_id: u8,
  pub norm_sum: [AccT; NORM_STAT_IDS],
  pub norm_running_max: [AccT; NORM_STAT_IDS],
  pub norm_max: [AccT; NORM_STAT_IDS],
  pub norm_count: [AccT; NORM_STAT_IDS],
  pub norm_mean: [AccT; NORM_STAT_IDS],
  pub norm_inv_stddev: [AccScaleT; NORM_STAT_IDS],
  pub norm_inv_sum_exp: [AccScaleT; NORM_STAT_IDS],
  pub norm_reset: [bool; NORM_STAT_IDS],

  // Counter state
  pub counter_val: [u32; NUM_COUNTERS],
  pub counter_snapshot_val: [u32; NUM_COUNTERS],
  pub counter_config: [u16; NUM_COUNTERS],
  pub counter_external: [u32; NUM_EXTERNAL_COUNTERS],
  pub counter_external_flag: [bool; NUM_COUNTERS],
  pub snapshot_enable: bool,
  pub op_in_progress: bool,

  // Memory structures
  pub spad: Vec<Vec<ElemT>>,
  pub pe_state: Vec<Vec<AccT>>,
  pub accumulator: Vec<Vec<AccT>>,

  // CISC state
  pub a_addr: RegT,
  pub b_addr: RegT,
  pub c_addr: RegT,
  pub d_addr: RegT,
  pub m: RegT,
  pub n: RegT,
  pub k: RegT,
  pub repeating_bias: bool,

  // DMA handlers for memory access
  pub dma_read: Option<Arc<Mutex<DmaReadHandler>>>,
  pub dma_write: Option<Arc<Mutex<DmaWriteHandler>>>,
}

impl GemminiState {
  pub fn new() -> Self {
    let mut state = Self {
      enable: true,
      resetted: false,

      output_sp_addr: 0,
      preload_sp_addr: 0,
      preload_cols: 0,
      preload_rows: 0,
      output_cols: 0,
      output_rows: 0,

      mode: Dataflow::OS,
      sys_act: Activation::NONE,
      acc_act: Activation::NONE,
      sys_shift: 0,
      sys_acc_shift: 0,

      load_strides: [0; LOAD_STATES],
      store_stride: 0,
      load_block_strides: [0; LOAD_STATES],
      load_shrunks: [false; LOAD_STATES],
      load_scales: [MVIN_SCALE_IDENTITY; LOAD_STATES],
      pixels_per_rows: [1; LOAD_STATES],
      acc_shift: 1.0,
      c_stride: 0,
      a_stride: 0,

      pool_stride: 0,
      pool_size: 0,
      pool_out_dim: 0,
      pool_porows: 0,
      pool_pocols: 0,
      pool_orows: 0,
      pool_ocols: 0,
      pool_lpad: 0,
      pool_upad: 0,

      a_transpose: false,
      b_transpose: false,

      loop_ws_I: 0,
      loop_ws_J: 0,
      loop_ws_K: 0,
      loop_ws_pad_I: 0,
      loop_ws_pad_J: 0,
      loop_ws_pad_K: 0,
      loop_ws_A: 0,
      loop_ws_B: 0,
      loop_ws_D: 0,
      loop_ws_C: 0,
      loop_ws_A_stride: 0,
      loop_ws_B_stride: 0,
      loop_ws_D_stride: 0,
      loop_ws_C_stride: 0,

      loop_conv_ws_batch_size: 0,
      loop_conv_ws_in_row_dim: 0,
      loop_conv_ws_in_col_dim: 0,
      loop_conv_ws_in_channels: 0,
      loop_conv_ws_out_channels: 0,
      loop_conv_ws_in_stride: 0,
      loop_conv_ws_weight_stride: 0,
      loop_conv_ws_out_stride: 0,
      loop_conv_ws_out_row_dim: 0,
      loop_conv_ws_pool_out_row_dim: 0,
      loop_conv_ws_out_col_dim: 0,
      loop_conv_ws_pool_out_col_dim: 0,
      loop_conv_ws_stride: 0,
      loop_conv_ws_padding: 0,
      loop_conv_ws_kernel_dim: 0,
      loop_conv_ws_pool_size: 0,
      loop_conv_ws_pool_stride: 0,
      loop_conv_ws_pool_padding: 0,
      loop_conv_ws_batches: 0,
      loop_conv_ws_porows: 0,
      loop_conv_ws_pocols: 0,
      loop_conv_ws_pochs: 0,
      loop_conv_ws_krows: 0,
      loop_conv_ws_kcols: 0,
      loop_conv_ws_kchs: 0,
      loop_conv_ws_lpad: 0,
      loop_conv_ws_rpad: 0,
      loop_conv_ws_upad: 0,
      loop_conv_ws_dpad: 0,
      loop_conv_ws_plpad: 0,
      loop_conv_ws_prad: 0,
      loop_conv_ws_pupad: 0,
      loop_conv_ws_pdpad: 0,
      loop_conv_ws_orows: 0,
      loop_conv_ws_ocols: 0,
      loop_conv_ws_kernel_dilation: 0,
      loop_conv_ws_input: 0,
      loop_conv_ws_weights: 0,
      loop_conv_ws_output: 0,
      loop_conv_ws_bias: 0,

      igelu_qb: 0,
      igelu_qc: 0,
      qln2: 0,
      qln2_inv: 0,
      norm_stat_id: 0,
      norm_sum: [0; NORM_STAT_IDS],
      norm_running_max: [i32::MIN; NORM_STAT_IDS],
      norm_max: [0; NORM_STAT_IDS],
      norm_count: [0; NORM_STAT_IDS],
      norm_mean: [0; NORM_STAT_IDS],
      norm_inv_stddev: [0.0; NORM_STAT_IDS],
      norm_inv_sum_exp: [0.0; NORM_STAT_IDS],
      norm_reset: [true; NORM_STAT_IDS],

      counter_val: [0; NUM_COUNTERS],
      counter_snapshot_val: [0; NUM_COUNTERS],
      counter_config: [0; NUM_COUNTERS],
      counter_external: [0; NUM_EXTERNAL_COUNTERS],
      counter_external_flag: [false; NUM_COUNTERS],
      snapshot_enable: false,
      op_in_progress: false,

      spad: vec![vec![0; DIM]; SP_MATRICES * DIM],
      pe_state: vec![vec![0; DIM]; DIM],
      accumulator: vec![vec![0; DIM]; ACCUM_ROWS],

      a_addr: 0,
      b_addr: 0,
      c_addr: 0,
      d_addr: 0,
      m: 0,
      n: 0,
      k: 0,
      repeating_bias: false,

      dma_read: None,
      dma_write: None,
    };

    state.reset();
    state
  }

  pub fn reset(&mut self) {
    self.enable = true;

    self.spad.clear();
    self.spad.resize(SP_MATRICES * DIM, vec![0; DIM]);

    self.pe_state.clear();
    self.pe_state.resize(DIM, vec![0; DIM]);

    self.accumulator.clear();
    self.accumulator.resize(ACCUM_ROWS, vec![0; DIM]);

    // CISC reset
    self.a_addr = 0;
    self.b_addr = 0;
    self.c_addr = 0;
    self.d_addr = 0;
    self.m = 0;
    self.n = 0;
    self.k = 0;
    self.repeating_bias = false;

    // Norm reset
    for i in 0..NORM_STAT_IDS {
      self.norm_reset[i] = true;
    }

    // Dummy counter reset
    self.snapshot_enable = false;
    self.op_in_progress = false;

    self.resetted = true;

    log::info!("Gemmini extension configured with:");
    log::info!("    dim = {}", DIM);
  }
}

pub struct Gemmini {
  pub state: GemminiState,

  // Function codes
  config_funct: u64,
  mvin_funct: u64,
  mvin2_funct: u64,
  mvin3_funct: u64,
  mvout_funct: u64,
  compute_preloaded_funct: u64,
  compute_accumulated_funct: u64,
  preload_funct: u64,
  flush_funct: u64,
  loop_ws_funct: u64,
  loop_ws_config_bounds_funct: u64,
  loop_ws_config_addrs_AB_funct: u64,
  loop_ws_config_addrs_DC_funct: u64,
  loop_ws_config_strides_AB_funct: u64,
  loop_ws_config_strides_DC_funct: u64,
  loop_conv_ws_funct: u64,
  loop_conv_ws_config_1_funct: u64,
  loop_conv_ws_config_2_funct: u64,
  loop_conv_ws_config_3_funct: u64,
  loop_conv_ws_config_4_funct: u64,
  loop_conv_ws_config_5_funct: u64,
  loop_conv_ws_config_6_funct: u64,
  fence_funct: u64,
  counter_op_funct: u64,
}

impl Gemmini {
  pub fn new() -> Self {
    Self {
      state: GemminiState::new(),
      config_funct: 0,
      mvin_funct: 2,
      mvin2_funct: 1,
      mvin3_funct: 14,
      mvout_funct: 3,
      compute_preloaded_funct: 4,
      compute_accumulated_funct: 5,
      preload_funct: 6,
      flush_funct: 7,
      loop_ws_funct: 8,
      loop_ws_config_bounds_funct: 9,
      loop_ws_config_addrs_AB_funct: 10,
      loop_ws_config_addrs_DC_funct: 11,
      loop_ws_config_strides_AB_funct: 12,
      loop_ws_config_strides_DC_funct: 13,
      loop_conv_ws_funct: 15,
      loop_conv_ws_config_1_funct: 16,
      loop_conv_ws_config_2_funct: 17,
      loop_conv_ws_config_3_funct: 18,
      loop_conv_ws_config_4_funct: 19,
      loop_conv_ws_config_5_funct: 20,
      loop_conv_ws_config_6_funct: 21,
      fence_funct: 127,
      counter_op_funct: 126,
    }
  }

  pub fn reset(&mut self) {
    self.state.reset();
  }

  pub fn set_dma_handlers(&mut self, dma_read: Arc<Mutex<DmaReadHandler>>, dma_write: Arc<Mutex<DmaWriteHandler>>) {
    self.state.dma_read = Some(dma_read);
    self.state.dma_write = Some(dma_write);
  }

  pub fn execute(&mut self, funct: u64, xs1: RegT, xs2: RegT) -> RegT {
    if !self.state.resetted {
      self.reset();
    }

    if self.state.op_in_progress {
      // Counter increment would happen here
    }

    if funct == self.mvin_funct {
      self.mvin(xs1, xs2, 0);
    } else if funct == self.mvin2_funct {
      self.mvin(xs1, xs2, 1);
    } else if funct == self.mvin3_funct {
      self.mvin(xs1, xs2, 2);
    } else if funct == self.mvout_funct {
      self.mvout(xs1, xs2);
    } else if funct == self.preload_funct {
      self.preload(xs1, xs2);
    } else if funct == self.config_funct {
      self.config(xs1, xs2);
    } else if funct == self.compute_preloaded_funct {
      self.compute(xs1, xs2, true);
    } else if funct == self.compute_accumulated_funct {
      self.compute(xs1, xs2, false);
    } else if funct == self.loop_ws_config_bounds_funct {
      self.loop_ws_config_bounds(xs1, xs2);
    } else if funct == self.loop_ws_config_addrs_AB_funct {
      self.loop_ws_config_addrs_AB(xs1, xs2);
    } else if funct == self.loop_ws_config_addrs_DC_funct {
      self.loop_ws_config_addrs_DC(xs1, xs2);
    } else if funct == self.loop_ws_config_strides_AB_funct {
      self.loop_ws_config_strides_AB(xs1, xs2);
    } else if funct == self.loop_ws_config_strides_DC_funct {
      self.loop_ws_config_strides_DC(xs1, xs2);
    } else if funct == self.loop_ws_funct {
      self.loop_ws(xs1, xs2);
    } else if funct == self.loop_conv_ws_config_1_funct {
      self.loop_conv_ws_config_1(xs1, xs2);
    } else if funct == self.loop_conv_ws_config_2_funct {
      self.loop_conv_ws_config_2(xs1, xs2);
    } else if funct == self.loop_conv_ws_config_3_funct {
      self.loop_conv_ws_config_3(xs1, xs2);
    } else if funct == self.loop_conv_ws_config_4_funct {
      self.loop_conv_ws_config_4(xs1, xs2);
    } else if funct == self.loop_conv_ws_config_5_funct {
      self.loop_conv_ws_config_5(xs1, xs2);
    } else if funct == self.loop_conv_ws_config_6_funct {
      self.loop_conv_ws_config_6(xs1, xs2);
    } else if funct == self.loop_conv_ws_funct {
      self.loop_conv_ws(xs1, xs2);
    } else if funct == self.counter_op_funct {
      return self.counter_operation(xs1);
    } else if funct == self.flush_funct {
      log::info!("GEMMINI: flush");
    } else if funct == self.fence_funct {
      log::info!("GEMMINI: fence");
    } else {
      log::error!("GEMMINI: encountered unknown instruction with funct: {}", funct);
    }

    self.state.op_in_progress = funct != self.flush_funct;
    0
  }

  // Helper functions for DRAM access via DMA
  fn read_from_dram<T: Copy>(&self, addr: RegT) -> T {
    let size = std::mem::size_of::<T>();

    if let Some(ref dma_read) = self.state.dma_read {
      let mut handler = dma_read.lock().unwrap();
      match handler.read(addr, size as u32) {
        Ok(data) => {
          let mut bytes = vec![0u8; size];
          for i in 0..size {
            bytes[i] = ((data >> (i * 8)) & 0xFF) as u8;
          }
          unsafe { std::ptr::read(bytes.as_ptr() as *const T) }
        },
        Err(_) => unsafe { std::mem::zeroed() },
      }
    } else {
      // No DMA handler available, return zero
      unsafe { std::mem::zeroed() }
    }
  }

  fn write_to_dram<T: Copy>(&mut self, addr: RegT, data: T) {
    let size = std::mem::size_of::<T>();
    let bytes = unsafe { std::slice::from_raw_parts(&data as *const T as *const u8, size) };

    if let Some(ref dma_write) = self.state.dma_write {
      let mut data_u128: u128 = 0;
      for i in 0..size.min(16) {
        data_u128 |= (bytes[i] as u128) << (i * 8);
      }

      let mut handler = dma_write.lock().unwrap();
      let _ = handler.write(addr, data_u128, size as u32);
    }
  }

  // Batch read DIM bytes from DRAM (optimized for DIM-sized chunks)
  fn read_batch_dim(&self, addr: RegT) -> [u8; DIM] {
    let mut result = [0u8; DIM];

    if let Some(ref dma_read) = self.state.dma_read {
      let mut handler = dma_read.lock().unwrap();

      match handler.read(addr, DIM as u32) {
        Ok(data) => {
          for i in 0..DIM {
            result[i] = ((data >> (i * 8)) & 0xFF) as u8;
          }
        },
        Err(_) => {
          // Return zeros on error
        }
      }
    }

    result
  }

  // Batch write DIM bytes to DRAM (optimized for DIM-sized chunks)
  fn write_batch_dim(&mut self, addr: RegT, data: &[u8; DIM]) {
    if let Some(ref dma_write) = self.state.dma_write {
      let mut handler = dma_write.lock().unwrap();

      let mut data_u128: u128 = 0;
      for i in 0..DIM {
        data_u128 |= (data[i] as u128) << (i * 8);
      }

      let _ = handler.write(addr, data_u128, DIM as u32);
    }
  }

  fn read_matrix_from_dram(
    &self,
    addr: RegT,
    rows: RegT,
    cols: RegT,
    zeroable: bool,
    repeating_bias: bool,
  ) -> Vec<Vec<ElemT>> {
    let mut result = vec![vec![0; cols as usize]; rows as usize];

    if addr == 0 {
      if zeroable {
        return result;
      }
      panic!("ERROR: non-zeroable matrix given address zero!");
    }

    // Batch read optimization: read DIM bytes at a time
    for i in 0..rows as usize {
      let ii = if repeating_bias { 0 } else { i };
      let dram_row_addr = addr + (ii * cols as usize * std::mem::size_of::<ElemT>()) as u64;

      // Read in DIM-byte chunks
      for j in (0..cols as usize).step_by(DIM) {
        let remaining = cols as usize - j;
        if remaining >= DIM {
          // Read full DIM bytes
          let bytes = self.read_batch_dim(dram_row_addr + j as u64);
          for k in 0..DIM {
            result[i][j + k] = bytes[k] as ElemT;
          }
        } else {
          // Handle remaining bytes individually (fallback for tail)
          for k in 0..remaining {
            result[i][j + k] = self.read_from_dram::<ElemT>(dram_row_addr + (j + k) as u64);
          }
        }
      }
    }

    result
  }

  // Helper functions for bit conversions
  fn scale_t_to_scale_t_bits(scale: ScaleT) -> u32 {
    scale.to_bits()
  }

  fn scale_t_bits_to_scale_t(bits: u32) -> ScaleT {
    f32::from_bits(bits)
  }

  fn acc_scale_t_to_acc_scale_t_bits(scale: AccScaleT) -> u32 {
    scale.to_bits()
  }

  fn acc_scale_t_bits_to_acc_scale_t(bits: u32) -> AccScaleT {
    f32::from_bits(bits)
  }

  // Rounding right shift
  fn rounding_right_shift(x: AccT, shift: i32) -> AccT {
    if shift > 0 {
      let shifted = x >> shift;
      let round_bit = if shift > 0 { (x >> (shift - 1)) & 1 } else { 0 };
      let sticky_bits = if shift > 1 { x & ((1 << (shift - 1)) - 1) } else { 0 };
      let round_up = round_bit & ((sticky_bits != 0) as i32 | (shifted & 1));
      shifted + round_up
    } else if shift < 0 {
      x << (-shift)
    } else {
      x
    }
  }

  fn round_near_even(x: f32) -> i32 {
    let i = x as i64;
    let next = if x < 0.0 { i - 1 } else { i + 1 };
    let mut rem = x - i as f32;
    rem = rem.abs();
    if rem < 0.5 {
      i as i32
    } else if rem > 0.5 {
      next as i32
    } else {
      if i % 2 == 0 {
        i as i32
      } else {
        next as i32
      }
    }
  }

  // Activation functions
  fn apply_activation(value: ElemT, act: Activation) -> ElemT {
    match act {
      Activation::RELU => {
        if value > 0 {
          value
        } else {
          0
        }
      },
      _ => value,
    }
  }

  fn apply_activation_sys(&self, value: ElemT) -> ElemT {
    Self::apply_activation(value, self.state.sys_act)
  }

  fn apply_activation_acc(&self, value: ElemT) -> ElemT {
    Self::apply_activation(value, self.state.acc_act)
  }

  fn apply_igelu(q: AccT, qb: AccT, qc: AccT) -> AccT {
    let q_sign = if q < 0 { -1 } else { 1 };
    let q_abs = q.abs();
    let q_clipped = if q_abs > -qb { -qb } else { q_abs };
    let q_poly = (q_clipped + qb) * (q_clipped + qb) + qc;
    let q_erf = q_sign * q_poly;
    q * (q_erf + qc)
  }

  fn apply_iexp(q: AccT, qb: AccT, qc: AccT, qln2: AccT, qln2_inv: AccT) -> AccT {
    let z = (-q * qln2_inv) / (1 << 16);
    let qp = q + z * qln2;
    let q_exp = (qp + qb) * (qp + qb) + qc;
    q_exp >> z
  }

  fn apply_pre_activation_acc(&self, value: AccT) -> AccT {
    match self.state.acc_act {
      Activation::IGELU => Self::apply_igelu(value, self.state.igelu_qb, self.state.igelu_qc),
      Activation::LAYERNORM => {
        let stat_id = self.state.norm_stat_id as usize;
        let norm_mean = self.state.norm_mean[stat_id];
        let norm_inv_stddev = self.state.norm_inv_stddev[stat_id];
        let scaled = Self::round_near_even((value - norm_mean) as f32 * norm_inv_stddev);
        scaled.max(i32::MIN).min(i32::MAX)
      },
      Activation::SOFTMAX => {
        let stat_id = self.state.norm_stat_id as usize;
        let norm_max = self.state.norm_max[stat_id];
        let norm_inv_sum_exp = self.state.norm_inv_sum_exp[stat_id];
        let exp_val = Self::apply_iexp(
          value - norm_max,
          self.state.igelu_qb,
          self.state.igelu_qc,
          self.state.qln2,
          self.state.qln2_inv,
        );
        let scaled = Self::round_near_even(exp_val as f32 * norm_inv_sum_exp);
        scaled.max(i32::MIN).min(i32::MAX)
      },
      _ => value,
    }
  }

  fn acc_scale(value: AccT, scale: AccScaleT) -> ElemT {
    let y = Self::round_near_even(value as f32 * scale);
    y.max(ELEM_T_MIN as i32).min(ELEM_T_MAX as i32) as ElemT
  }

  fn mvin_scale(value: ElemT, scale: ScaleT) -> ElemT {
    let y = Self::round_near_even(value as f32 * scale);
    y.max(ELEM_T_MIN as i32).min(ELEM_T_MAX as i32) as ElemT
  }

  fn sys_shift(value: AccT, shift: i32) -> ElemT {
    let shifted = Self::rounding_right_shift(value, shift);
    shifted.max(ELEM_T_MIN as i32).min(ELEM_T_MAX as i32) as ElemT
  }

  // Normalization functions
  fn non_terminating_norm_cmd(cmd: NormCmd) -> NormCmd {
    match cmd {
      NormCmd::RESET => NormCmd::RESET,
      NormCmd::MEAN => NormCmd::SUM,
      NormCmd::INV_STDDEV => NormCmd::VARIANCE,
      NormCmd::INV_SUM_EXP => NormCmd::SUM_EXP,
      _ => cmd,
    }
  }

  fn apply_norm(&mut self, x: &[AccT], len: usize, cmd: NormCmd) -> bool {
    let stat_id = self.state.norm_stat_id as usize;

    if self.state.norm_reset[stat_id] {
      self.state.norm_sum[stat_id] = 0;
      self.state.norm_count[stat_id] = 0;
      self.state.norm_running_max[stat_id] = i32::MIN;
    }

    self.state.norm_reset[stat_id] = matches!(cmd, NormCmd::RESET | NormCmd::MEAN | NormCmd::INV_STDDEV);

    match cmd {
      NormCmd::SUM | NormCmd::MEAN => {
        for i in 0..len {
          self.state.norm_sum[stat_id] += x[i];
        }
        self.state.norm_count[stat_id] += len as AccT;
      },
      NormCmd::VARIANCE | NormCmd::INV_STDDEV => {
        let norm_mean = self.state.norm_mean[stat_id];
        for i in 0..len {
          let diff = x[i] - norm_mean;
          self.state.norm_sum[stat_id] += diff * diff;
        }
        self.state.norm_count[stat_id] += len as AccT;
      },
      NormCmd::MAX => {
        for i in 0..len {
          if x[i] > self.state.norm_running_max[stat_id] {
            self.state.norm_running_max[stat_id] = x[i];
          }
        }
      },
      NormCmd::SUM_EXP | NormCmd::INV_SUM_EXP => {
        self.state.norm_max[stat_id] = self.state.norm_running_max[stat_id];
        for i in 0..len {
          self.state.norm_sum[stat_id] += Self::apply_iexp(
            x[i] - self.state.norm_max[stat_id],
            self.state.igelu_qb,
            self.state.igelu_qc,
            self.state.qln2,
            self.state.qln2_inv,
          );
        }
      },
      _ => {},
    }

    if cmd == NormCmd::MEAN {
      self.state.norm_mean[stat_id] = self.state.norm_sum[stat_id] / self.state.norm_count[stat_id];
    } else if cmd == NormCmd::INV_STDDEV {
      let variance = self.state.norm_sum[stat_id] / self.state.norm_count[stat_id];
      let norm_stddev = (variance as f64).sqrt();
      let norm_stddev = if variance == 0 { 1.0 } else { norm_stddev };
      self.state.norm_inv_stddev[stat_id] = 1.0 / norm_stddev as f32;
    } else if cmd == NormCmd::INV_SUM_EXP {
      self.state.norm_running_max[stat_id] = i32::MIN;
      self.state.norm_inv_sum_exp[stat_id] = 127.0 / self.state.norm_sum[stat_id] as f32;
    }

    cmd == NormCmd::RESET
  }

  // Core Gemmini operations
  pub fn mvin(&mut self, dram_addr: RegT, sp_addr: RegT, state_id: usize) {
    let accumulator = ((sp_addr >> 31) & 0x1) != 0;
    let accumulate = ((sp_addr >> 30) & 0x1) != 0;
    let base_row_addr = (sp_addr & 0x1FFFFFFF) as usize;
    let cols = ((sp_addr >> ADDR_LEN) & 0xFFFF) as usize;
    let rows = ((sp_addr >> (ADDR_LEN + 16)) & 0xFFFF) as usize;

    let is_zeros = dram_addr == 0;

    let load_stride = self.state.load_strides[state_id];
    let load_block_stride = self.state.load_block_strides[state_id] as usize;
    let load_scale = self.state.load_scales[state_id];
    let pixels_per_row = self.state.pixels_per_rows[state_id] as usize;

    log::info!(
      "GEMMINI: mvin - 0x{:02x} cols and 0x{:02x} rows from 0x{:08x} to addr 0x{:08x}",
      cols,
      rows,
      dram_addr,
      sp_addr as u32
    );

    for row in 0..rows {
      let dram_row_addr = dram_addr + (row as u64 * load_stride);

      for col in 0..cols {
        let block = col / DIM;
        let spad_col = col % DIM;
        let spad_row = base_row_addr + row + block * load_block_stride;

        for pixel in 0..pixels_per_row {
          if pixel > spad_row {
            break;
          }

          if accumulator {
            let dram_byte_addr = dram_row_addr + (col * std::mem::size_of::<ElemT>()) as u64;

            let value: AccT = if is_zeros {
              0
            } else {
              let elem_value = self.read_from_dram::<ElemT>(dram_byte_addr);
              Self::mvin_scale(elem_value, load_scale) as AccT
            };

            if accumulate {
              self.state.accumulator[spad_row - pixel][spad_col + pixel * cols] += value;
            } else {
              self.state.accumulator[spad_row - pixel][spad_col + pixel * cols] = value;
            }
          } else {
            let dram_byte_addr = dram_row_addr + (col * std::mem::size_of::<ElemT>()) as u64;

            let value: ElemT = if is_zeros {
              0
            } else {
              let elem_value = self.read_from_dram::<ElemT>(dram_byte_addr);
              Self::mvin_scale(elem_value, load_scale)
            };

            self.state.spad[spad_row - pixel][spad_col + pixel * cols] = value;
          }
        }
      }
    }
  }

  pub fn mvout(&mut self, dram_addr: RegT, sp_addr: RegT) {
    let accumulator = ((sp_addr >> 31) & 0x1) != 0;
    let full = ((sp_addr >> 29) & 0x1) != 0;
    let norm_cmd_bits = ((sp_addr >> 26) & 0x7) as u8;
    let norm_cmd = match norm_cmd_bits {
      0 => NormCmd::RESET,
      1 => NormCmd::SUM,
      2 => NormCmd::MEAN,
      3 => NormCmd::VARIANCE,
      4 => NormCmd::INV_STDDEV,
      5 => NormCmd::MAX,
      6 => NormCmd::SUM_EXP,
      7 => NormCmd::INV_SUM_EXP,
      _ => NormCmd::RESET,
    };
    let base_row_addr = (sp_addr & 0x3FFFFFF) as usize;
    let cols = ((sp_addr >> ADDR_LEN) & 0xFFFF) as usize;
    let rows = ((sp_addr >> (ADDR_LEN + 16)) & 0xFFFF) as usize;

    let block_stride = DIM;

    log::info!(
      "GEMMINI: mvout - 0x{:02x} cols and 0x{:02x} rows from 0x{:08x} to addr 0x{:08x}",
      cols,
      rows,
      sp_addr as u32,
      dram_addr
    );

    if self.state.pool_stride == 0 {
      // No pooling
      for i in 0..rows {
        let dram_row_addr = dram_addr + (i as u64 * self.state.store_stride);

        let mut should_write = true;
        for j in (0..cols).step_by(DIM) {
          let block = j / DIM;
          let spad_row = base_row_addr + block * block_stride + i;
          let len = if cols - j > DIM { DIM } else { cols - j };

          let is_last = j + DIM >= cols;
          let n_cmd = if is_last {
            norm_cmd
          } else {
            Self::non_terminating_norm_cmd(norm_cmd)
          };

          // Copy the row data to avoid borrow checker issues
          let row_data: Vec<AccT> = self.state.accumulator[spad_row][0..DIM].to_vec();
          should_write = self.apply_norm(&row_data, len, n_cmd);
        }

        if !should_write {
          continue;
        }

        for j in 0..cols {
          let block = j / DIM;
          let spad_col = j % DIM;
          let spad_row = base_row_addr + block * block_stride + i;

          if accumulator {
            let acc_value = self.state.accumulator[spad_row][spad_col];
            let acc_value_pre = self.apply_pre_activation_acc(acc_value);
            let shifted = Self::acc_scale(acc_value_pre, self.state.acc_shift);
            let activated = self.apply_activation_acc(shifted);

            let sizeof_output = if full {
              std::mem::size_of::<AccT>()
            } else {
              std::mem::size_of::<ElemT>()
            };

            let dram_byte_addr = dram_row_addr + (j * sizeof_output) as u64;

            if full {
              self.write_to_dram(dram_byte_addr, acc_value);
            } else {
              self.write_to_dram(dram_byte_addr, activated);
            }
          } else {
            let dram_byte_addr = dram_row_addr + (j * std::mem::size_of::<ElemT>()) as u64;
            let value = self.state.spad[spad_row][spad_col];
            self.write_to_dram(dram_byte_addr, value);
          }
        }
      }
    } else {
      // Perform pooling
      let pool_stride = self.state.pool_stride as usize;
      let pool_size = self.state.pool_size as usize;
      let pool_out_dim = self.state.pool_out_dim as usize;
      let porows = self.state.pool_porows as usize;
      let pocols = self.state.pool_pocols as usize;
      let orows = self.state.pool_orows as usize;
      let ocols = self.state.pool_ocols as usize;
      let plpad = self.state.pool_lpad as i32;
      let pupad = self.state.pool_upad as i32;
      let channels = cols;

      for porow in 0..porows {
        for pocol in 0..pocols {
          for poch in 0..channels {
            let mut value = ELEM_T_MIN;

            for wrow in 0..pool_size {
              for wcol in 0..pool_size {
                let orow = (porow * pool_stride + wrow) as i32 - pupad;
                let ocol = (pocol * pool_stride + wcol) as i32 - plpad;

                let row_addr = base_row_addr + (orow * ocols as i32 + ocol) as usize;

                let elem = if orow < 0 || ocol < 0 || orow >= orows as i32 || ocol >= ocols as i32 {
                  0
                } else if accumulator {
                  let acc_value = self.state.accumulator[row_addr][poch];
                  let shifted = Self::acc_scale(acc_value, self.state.acc_shift);
                  self.apply_activation_acc(shifted)
                } else {
                  self.state.spad[row_addr][poch]
                };

                if elem > value {
                  value = elem;
                }
              }
            }

            let dram_byte_addr = dram_addr
              + ((porow * pool_out_dim + pocol) as u64 * self.state.store_stride)
              + (poch * std::mem::size_of::<ElemT>()) as u64;
            self.write_to_dram(dram_byte_addr, value);
          }
        }
      }
    }
  }

  pub fn preload(&mut self, bd_addr: RegT, c_addr: RegT) {
    self.state.preload_sp_addr = (bd_addr & 0xFFFFFFFF) as u32;
    self.state.output_sp_addr = (c_addr & 0xFFFFFFFF) as u32;

    self.state.preload_cols = ((bd_addr >> ADDR_LEN) & 0xFFFF) as u16;
    self.state.preload_rows = ((bd_addr >> (ADDR_LEN + 16)) & 0xFFFF) as u16;
    self.state.output_cols = ((c_addr >> ADDR_LEN) & 0xFFFF) as u16;
    self.state.output_rows = ((c_addr >> (ADDR_LEN + 16)) & 0xFFFF) as u16;

    log::info!(
      "GEMMINI: preload - scratchpad output addr = 0x{:08x}, scratchpad preload addr = 0x{:08x}",
      self.state.output_sp_addr,
      self.state.preload_sp_addr
    );
  }

  pub fn config(&mut self, rs1: RegT, rs2: RegT) {
    if (rs1 & 0b11) == 0 {
      // config_ex: configure execute pipeline
      let rs1_2 = (rs1 >> 2) & 0b1;
      let new_mode = if rs1_2 == 0 { Dataflow::OS } else { Dataflow::WS };

      let rs1_4_3 = (rs1 >> 3) & 0b11;
      let new_act = match rs1_4_3 {
        0 => Activation::NONE,
        1 => Activation::RELU,
        2 => Activation::LAYERNORM,
        3 => Activation::IGELU,
        _ => Activation::NONE,
      };

      let new_sys_shift = rs2 & 0xFFFFFFFF;
      let new_sys_acc_shift = (rs1 >> 32) & 0xFFFFFFFF;
      let new_c_stride = ((rs2 >> 48) & 0xFFFF) as u16;
      let new_a_stride = ((rs1 >> 16) & 0xFFFF) as u16;
      let new_a_transpose = ((rs1 >> 8) & 0x1) != 0;
      let new_b_transpose = ((rs1 >> 9) & 0x1) != 0;

      let set_only_strides = ((rs1 >> 7) & 0x1) != 0;

      if !set_only_strides {
        self.state.mode = new_mode;
        self.state.sys_act = new_act;
        self.state.sys_shift = new_sys_shift;
        self.state.sys_acc_shift = new_sys_acc_shift;
        self.state.a_transpose = new_a_transpose;
        self.state.b_transpose = new_b_transpose;
      }

      self.state.c_stride = new_c_stride;
      self.state.a_stride = new_a_stride;

      log::info!(
        "GEMMINI: config_ex - set mode to {:?}, activation to {:?}, sys shift to {:?}, sys acc shift to {:?}, a transpose to {:?}, b transpose to {:?}",
        new_mode,
        new_act,
        new_sys_shift,
        new_sys_acc_shift,
        new_a_transpose,
        new_b_transpose
      );
    } else if (rs1 & 0b11) == 1 {
      // config_mvin: configure load pipeline
      let state_id = ((rs1 >> 3) & 0x3) as usize;
      self.state.load_strides[state_id] = rs2;
      self.state.load_block_strides[state_id] = ((rs1 >> 16) & 0xFFFF) as u16;
      self.state.load_scales[state_id] = f32::from_bits(((rs1 >> 32) & 0xFFFFFFFF) as u32);
      self.state.pixels_per_rows[state_id] = ((rs1 >> 8) & 0xFF) as u8;

      if self.state.pixels_per_rows[state_id] == 0 {
        self.state.pixels_per_rows[state_id] = 1;
      }

      log::info!(
        "GEMMINI: config_ld - set load stride to {:?} (rs2=0x{:x}), load block stride to {:?}, load scale to {:?}, pixels per rows to {:?}",
        rs2,
        rs2,
        self.state.load_block_strides[state_id],
        self.state.load_scales[state_id],
        self.state.pixels_per_rows[state_id]
      );
    } else if (rs1 & 0b11) == 2 {
      // config_mvout: configure store pipeline
      self.state.store_stride = rs2 & 0xFFFFFFFF;

      let rs1_3_2 = (rs1 >> 2) & 0b11;
      let new_act = match rs1_3_2 {
        0 => Activation::NONE,
        1 => Activation::RELU,
        2 => Activation::LAYERNORM,
        3 => Activation::IGELU,
        _ => Activation::NONE,
      };
      self.state.acc_act = new_act;

      let new_acc_shift = (rs2 >> 32) & 0xFFFFFFFF;
      self.state.acc_shift = f32::from_bits(new_acc_shift as u32);

      self.state.pool_stride = ((rs1 >> 4) & 0x3) as u8;
      self.state.pool_size = ((rs1 >> 6) & 0x3) as u8;
      self.state.pool_upad = ((rs1 >> 8) & 0x3) as u8;
      self.state.pool_lpad = ((rs1 >> 10) & 0x3) as u8;
      self.state.pool_out_dim = ((rs1 >> 24) & 0xFF) as u8;
      self.state.pool_porows = ((rs1 >> 32) & 0xFF) as u8;
      self.state.pool_pocols = ((rs1 >> 40) & 0xFF) as u8;
      self.state.pool_orows = ((rs1 >> 48) & 0xFF) as u8;
      self.state.pool_ocols = ((rs1 >> 56) & 0xFF) as u8;
    
      log::info!(
        "GEMMINI: config_st - set store stride to {:?}, activation to {:?}, acc shift to {:?}, pool stride to {:?}, pool size to {:?}, pool upad to {:?}, pool lpad to {:?}, pool out dim to {:?}, pool porows to {:?}, pool pocols to {:?}, pool orows to {:?}, pool ocols to {:?}",
        rs2 & 0xFFFFFFFF,
        new_act,
        f32::from_bits(new_acc_shift as u32),
        self.state.pool_stride,
        self.state.pool_size,
        self.state.pool_upad,
        self.state.pool_lpad,
        self.state.pool_out_dim,
        self.state.pool_porows,
        self.state.pool_pocols,
        self.state.pool_orows,
        self.state.pool_ocols
      );
    } else if (rs1 & 0b11) == 3 {
      // config_norm: configure norm pipeline
      self.state.norm_stat_id = ((rs1 >> 8) & 0xFF) as u8;
      if ((rs1 >> 17) & 1) == 0 {
        self.state.igelu_qb = (rs2 & 0xFFFFFFFF) as AccT;
        self.state.igelu_qc = ((rs2 >> 32) & 0xFFFFFFFF) as AccT;
        self.state.qln2 = ((rs1 >> 32) & 0xFFFFFFFF) as AccT;
      }

      log::info!(
        "GEMMINI: config_norm - set norm stat id to {:?}, igelu qb to {:?}, igelu qc to {:?}, qln2 to {:?}",
        self.state.norm_stat_id,
        self.state.igelu_qb,
        self.state.igelu_qc,
        self.state.qln2
      );
    }
  }

  pub fn compute(&mut self, a_addr: RegT, bd_addr: RegT, preload: bool) {
    let a_addr_real = (a_addr & 0xFFFFFFFF) as u32;
    let bd_addr_real = (bd_addr & 0xFFFFFFFF) as u32;

    let a_cols = ((a_addr >> ADDR_LEN) & 0xFFFF) as usize;
    let a_rows = ((a_addr >> (ADDR_LEN + 16)) & 0xFFFF) as usize;

    let bd_cols = ((bd_addr >> ADDR_LEN) & 0xFFFF) as usize;
    let bd_rows = ((bd_addr >> (ADDR_LEN + 16)) & 0xFFFF) as usize;

    log::info!(
      "GEMMINI: compute - preload = {}, scratchpad A addr = 0x{:08x}, scratchpad B addr 0x{:08x}",
      preload,
      a_addr_real,
      bd_addr_real
    );

    // Preload
    if preload {
      for i in 0..DIM {
        for j in 0..DIM {
          let preload_transpose = self.state.mode == Dataflow::WS && self.state.b_transpose;
          let r = if preload_transpose { j } else { i };
          let c = if preload_transpose { i } else { j };

          if i < self.state.preload_rows as usize && j < self.state.preload_cols as usize {
            let preload_value = if self.state.preload_sp_addr == !0 {
              0
            } else {
              self.state.spad[(self.state.preload_sp_addr as usize) + r][c]
            };
            self.state.pe_state[i][j] = preload_value as AccT;
          } else {
            self.state.pe_state[i][j] = 0;
          }
        }
      }
    }

    // Compute
    let mut results = vec![vec![0 as AccT; DIM]; DIM];
    for i in 0..DIM {
      for j in 0..DIM {
        if i < bd_rows && j < bd_cols {
          results[i][j] = if bd_addr_real == !0 {
            0
          } else {
            self.state.spad[(bd_addr_real as usize) + i][j] as AccT
          };
        } else {
          results[i][j] = 0;
        }
      }
    }

    for i in 0..DIM {
      for j in 0..DIM {
        for k in 0..DIM {
          let a = if a_addr_real != !0 {
            let r = if self.state.a_transpose { k } else { i } * self.state.a_stride as usize;
            let c = if self.state.a_transpose { i } else { k };

            if i < a_rows && k < a_cols {
              self.state.spad[(a_addr_real as usize) + r][c]
            } else {
              0
            }
          } else {
            0
          };

          if self.state.mode == Dataflow::WS {
            results[i][j] += (a as AccT) * self.state.pe_state[k][j];
          } else {
            let b = if bd_addr_real != !0 {
              let r = if self.state.b_transpose { j } else { k };
              let c = if self.state.b_transpose { k } else { j };

              if k < bd_rows && j < bd_cols {
                self.state.spad[(bd_addr_real as usize) + r][c]
              } else {
                0
              }
            } else {
              0
            };

            self.state.pe_state[i][j] += (a as AccT) * (b as AccT);
          }
        }
      }
    }

    // Write results
    if self.state.output_sp_addr != !0 {
      let acc = ((self.state.output_sp_addr >> 31) & 0x1) != 0;
      let acc_accum = ((self.state.output_sp_addr >> 30) & 0x1) != 0;
      let base_sp_addr = (self.state.output_sp_addr & 0x1FFFFFFF) as usize;

      for i in 0..self.state.output_rows as usize {
        for j in 0..self.state.output_cols as usize {
          let value = if self.state.mode == Dataflow::OS {
            self.state.pe_state[i][j]
          } else {
            results[i][j]
          };

          if acc {
            if acc_accum {
              self.state.accumulator[base_sp_addr + self.state.c_stride as usize * i][j] += value;
            } else {
              self.state.accumulator[base_sp_addr + self.state.c_stride as usize * i][j] = value;
            }
          } else {
            let shifted = if self.state.mode == Dataflow::OS {
              Self::sys_shift(value, self.state.sys_shift as i32)
            } else {
              Self::sys_shift(value, 0)
            };
            let activated = self.apply_activation_sys(shifted);
            self.state.spad[base_sp_addr + self.state.c_stride as usize * i][j] = activated;
          }
        }
      }
    }
  }

  pub fn compute_cisc(&mut self) {
    // Load operands from memory
    let a = self.read_matrix_from_dram(self.state.a_addr, self.state.m, self.state.k, false, false);
    let b = self.read_matrix_from_dram(self.state.b_addr, self.state.k, self.state.n, false, false);
    let d_matrix = if self.state.d_addr != 0 {
      self.read_matrix_from_dram(
        self.state.d_addr,
        self.state.m,
        self.state.n,
        true,
        self.state.repeating_bias,
      )
    } else {
      vec![vec![0 as ElemT; self.state.n as usize]; self.state.m as usize]
    };

    // Convert D matrix to AccT
    let mut d = vec![vec![0 as AccT; self.state.n as usize]; self.state.m as usize];
    for i in 0..self.state.m as usize {
      for j in 0..self.state.n as usize {
        d[i][j] = d_matrix[i][j] as AccT;
      }
    }

    // Initialize result
    let mut c = vec![vec![0 as ElemT; self.state.n as usize]; self.state.m as usize];

    // Multiply & apply activation
    for i in 0..self.state.m as usize {
      for j in 0..self.state.n as usize {
        let mut value = d[i][j];
        for k in 0..self.state.k as usize {
          value += (a[i][k] as AccT) * (b[k][j] as AccT);
        }
        let shifted = Self::acc_scale(value, self.state.acc_shift);
        let activated = self.apply_activation_acc(shifted);
        c[i][j] = activated;
      }
    }

    // Write back to memory
    for i in 0..self.state.m as usize {
      let dram_row_addr = self.state.c_addr + (i as u64 * std::mem::size_of::<ElemT>() as u64 * self.state.n);
      for j in 0..self.state.n as usize {
        let dram_byte_addr = dram_row_addr + (j * std::mem::size_of::<ElemT>()) as u64;
        self.write_to_dram(dram_byte_addr, c[i][j]);
      }
    }
  }

  pub fn counter_operation(&mut self, rs1: RegT) -> RegT {
    let counter_reset = (rs1 & 0x1) != 0;
    let snapshot_reset = ((rs1 >> 1) & 0x1) != 0;
    let take_snapshot = ((rs1 >> 2) & 0x1) != 0;
    let change_config = ((rs1 >> 3) & 0x1) != 0;
    let counter_index = ((rs1 >> 4) & 0x7) as usize;
    let counter_addr = ((rs1 >> 13) & 0x3F) as u16;
    let external_counter = ((rs1 >> 32) & 0x1) != 0;

    if counter_reset {
      for i in 0..NUM_COUNTERS {
        self.state.counter_val[i] = 0;
      }
      self.state.op_in_progress = false;
    }
    if snapshot_reset {
      self.state.snapshot_enable = false;
    }
    if take_snapshot {
      self.state.snapshot_enable = true;
      for i in 0..NUM_COUNTERS {
        if self.state.counter_external_flag[i] {
          self.state.counter_snapshot_val[i] = self.state.counter_external[self.state.counter_config[i] as usize];
        } else {
          self.state.counter_snapshot_val[i] = self.state.counter_val[i];
        }
      }
    }
    if change_config {
      self.state.counter_config[counter_index] = counter_addr;
      self.state.counter_val[counter_index] = 0;
      self.state.counter_external_flag[counter_index] = external_counter;
    }

    if self.state.snapshot_enable {
      self.state.counter_snapshot_val[counter_index] as RegT
    } else if self.state.counter_external_flag[counter_index] {
      self.state.counter_external[self.state.counter_config[counter_index] as usize] as RegT
    } else {
      self.state.counter_val[counter_index] as RegT
    }
  }

  // Loop WS configuration functions
  pub fn loop_ws_config_bounds(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_ws_I = (rs2 & 0xFFFF) as u16;
    self.state.loop_ws_J = ((rs2 >> 16) & 0xFFFF) as u16;
    self.state.loop_ws_K = ((rs2 >> 32) & 0xFFFF) as u16;

    self.state.loop_ws_pad_I = (rs1 & 0xFFFF) as u16;
    self.state.loop_ws_pad_J = ((rs1 >> 16) & 0xFFFF) as u16;
    self.state.loop_ws_pad_K = ((rs1 >> 32) & 0xFFFF) as u16;

    log::info!(
      "GEMMINI: loop_ws_config_bounds - set loop ws I to {:?}, loop ws J to {:?}, loop ws K to {:?}, loop ws pad I to {:?}, loop ws pad J to {:?}, loop ws pad K to {:?}",
      self.state.loop_ws_I,
      self.state.loop_ws_J,
      self.state.loop_ws_K,
      self.state.loop_ws_pad_I,
      self.state.loop_ws_pad_J,
      self.state.loop_ws_pad_K
    );
  }

  pub fn loop_ws_config_addrs_AB(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_ws_A = rs1;
    self.state.loop_ws_B = rs2;

    log::info!(
      "GEMMINI: loop_ws_config_addrs_AB - set loop ws A to {:?}, loop ws B to {:?}",
      self.state.loop_ws_A,
      self.state.loop_ws_B
    );
  }

  pub fn loop_ws_config_addrs_DC(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_ws_D = rs1;
    self.state.loop_ws_C = rs2;

    log::info!(
      "GEMMINI: loop_ws_config_addrs_DC - set loop ws D to {:?}, loop ws C to {:?}",
      self.state.loop_ws_D,
      self.state.loop_ws_C
    );
  }

  pub fn loop_ws_config_strides_AB(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_ws_A_stride = rs1;
    self.state.loop_ws_B_stride = rs2;

    log::info!(
      "GEMMINI: loop_ws_config_strides_AB - set loop ws A stride to {:?}, loop ws B stride to {:?}",
      self.state.loop_ws_A_stride,
      self.state.loop_ws_B_stride
    );
  }

  pub fn loop_ws_config_strides_DC(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_ws_D_stride = rs1;
    self.state.loop_ws_C_stride = rs2;

      log::info!(
        "GEMMINI: loop_ws_config_strides_DC - set loop ws D stride to {:?} (0x{:x}), loop ws C stride to {:?} (0x{:x})",
        self.state.loop_ws_D_stride,
        self.state.loop_ws_D_stride,
        self.state.loop_ws_C_stride,
        self.state.loop_ws_C_stride
      );
  }

  pub fn loop_ws(&mut self, rs1: RegT, rs2: RegT) {
    let ex_accumulate = (rs1 & 1) != 0;
    let full_c = ((rs1 >> 1) & 1) != 0;
    let low_d = ((rs1 >> 2) & 1) != 0;
    let act = ((rs1 >> 8) & 0x7) as u8;
    let a_transpose = (rs2 & 1) != 0;
    let b_transpose = ((rs2 >> 1) & 1) != 0;
    let a_spad_id = ((rs1 >> 18) & 0b11) as u8;
    let b_spad_id = ((rs1 >> 16) & 0b11) as u8;
    let is_resadd = ((rs2 >> 2) & 1) != 0;

    let i = self.state.loop_ws_I as usize;
    let j = self.state.loop_ws_J as usize;
    let k = self.state.loop_ws_K as usize;

    let pad_i = self.state.loop_ws_pad_I as usize;
    let pad_j = self.state.loop_ws_pad_J as usize;
    let pad_k = self.state.loop_ws_pad_K as usize;

    let garbage_addr: u32 = !0;

    let total_spad_rows = (i * k + k * j) * DIM;
    let total_acc_rows = (i * j) * DIM;

    if total_spad_rows > BANK_NUM * BANK_ROWS / 2 || total_acc_rows > ACC_ROWS / 2 {
      log::error!("LOOP_WS bounds were too large for double-buffering");
      return;
    }

    let mut a_sp_addr_start: u32 = 0;
    let mut b_sp_addr_start: u32 = ((BANK_NUM * BANK_ROWS / 2) - k * j * DIM) as u32;
    let d_sp_addr_start: u32 = 1 << (ADDR_LEN - 1);
    let c_sp_addr_start: u32 = (3 << (ADDR_LEN - 2)) | (if full_c { 1 << (ADDR_LEN - 3) } else { 0 });

    if a_spad_id == 2 {
      a_sp_addr_start = ((BANK_NUM * BANK_ROWS) / 2) as u32;
    }
    if b_spad_id == 2 {
      b_sp_addr_start = ((BANK_NUM * BANK_ROWS) - k * j * DIM) as u32;
    }

    if is_resadd {
      // Residual add implementation
      a_sp_addr_start = 1 << (ADDR_LEN - 1);
      b_sp_addr_start = 3 << (ADDR_LEN - 2);

      for ii in 0..i {
        for jj in 0..j {
          let a_sp_addr = a_sp_addr_start + ((ii * j + jj) * DIM) as u32;
          let b_sp_addr = b_sp_addr_start + ((ii * j + jj) * DIM) as u32;
          let c_sp_addr = c_sp_addr_start + ((ii * j + jj) * DIM) as u32;

          let dram_addr = self.state.loop_ws_A
            + ((ii * self.state.loop_ws_A_stride as usize + jj) * DIM * std::mem::size_of::<ElemT>()) as u64;
          let cols = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
          let rows = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
          self.mvin(dram_addr, (rows << 48) | (cols << 32) | a_sp_addr as u64, 0);

          let dram_addr = self.state.loop_ws_B
            + ((ii * self.state.loop_ws_B_stride as usize + jj) * DIM * std::mem::size_of::<ElemT>()) as u64;
          let cols = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
          let rows = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
          self.mvin(dram_addr, (rows << 48) | (cols << 32) | b_sp_addr as u64, 1);

          if self.state.loop_ws_C != 0 {
            let sizeof_c = if full_c {
              std::mem::size_of::<AccT>()
            } else {
              std::mem::size_of::<ElemT>()
            };
            let c_dram_addr =
              self.state.loop_ws_C + ((ii * self.state.loop_ws_C_stride as usize + jj) * DIM * sizeof_c) as u64;
            let c_cols = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
            let c_rows = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
            self.mvout(c_dram_addr, (c_rows << 48) | (c_cols << 32) | c_sp_addr as u64);
          }
        }
      }
      return;
    }

    // Load D (bias) if present
    if self.state.loop_ws_D != 0 {
      for ii in 0..i {
        for jj in 0..j {
          let sizeof_d = if low_d {
            std::mem::size_of::<ElemT>()
          } else {
            std::mem::size_of::<AccT>()
          };
          let dram_addr =
            self.state.loop_ws_D + ((ii * self.state.loop_ws_D_stride as usize + jj) * DIM * sizeof_d) as u64;
          let sp_addr = d_sp_addr_start + ((ii * j + jj) * DIM) as u32;
          let cols = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
          let rows = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
          self.mvin(dram_addr, (rows << 48) | (cols << 32) | sp_addr as u64, 2);
        }
      }
    }

    // Main computation loop
    for kk in 0..k {
      for jj in 0..j {
        for ii in 0..i {
          let a_sp_addr = if a_transpose {
            a_sp_addr_start + ((kk * i + ii) * DIM) as u32
          } else {
            a_sp_addr_start + ((ii * k + kk) * DIM) as u32
          };

          let b_sp_addr = if b_transpose {
            b_sp_addr_start + ((jj * k + kk) * DIM) as u32
          } else {
            b_sp_addr_start + ((kk * j + jj) * DIM) as u32
          };

          let c_sp_addr = c_sp_addr_start + ((ii * j + jj) * DIM) as u32;

          // Mvin A
          if jj == 0 && self.state.loop_ws_A != 0 {
            let (dram_addr, cols, rows) = if a_transpose {
              let addr = self.state.loop_ws_A
                + ((kk * self.state.loop_ws_A_stride as usize + ii) * DIM * std::mem::size_of::<ElemT>()) as u64;
              let c = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
              let r = (DIM - if kk == k - 1 { pad_k } else { 0 }) as u64;
              (addr, c, r)
            } else {
              let addr = self.state.loop_ws_A
                + ((ii * self.state.loop_ws_A_stride as usize + kk) * DIM * std::mem::size_of::<ElemT>()) as u64;
              let c = (DIM - if kk == k - 1 { pad_k } else { 0 }) as u64;
              let r = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
              (addr, c, r)
            };
            self.mvin(dram_addr, (rows << 48) | (cols << 32) | a_sp_addr as u64, 0);
          }

          // Mvin B
          if ii == 0 && self.state.loop_ws_B != 0 {
            let (dram_addr, cols, rows) = if b_transpose {
              let addr = self.state.loop_ws_B
                + ((jj * self.state.loop_ws_B_stride as usize + kk) * DIM * std::mem::size_of::<ElemT>()) as u64;
              let c = (DIM - if kk == k - 1 { pad_k } else { 0 }) as u64;
              let r = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
              (addr, c, r)
            } else {
              let addr = self.state.loop_ws_B
                + ((kk * self.state.loop_ws_B_stride as usize + jj) * DIM * std::mem::size_of::<ElemT>()) as u64;
              let c = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
              let r = (DIM - if kk == k - 1 { pad_k } else { 0 }) as u64;
              (addr, c, r)
            };
            self.mvin(dram_addr, (rows << 48) | (cols << 32) | b_sp_addr as u64, 1);
          }

          // Compute
          if !is_resadd {
            let mut pre_sp_addr = if ii == 0 { b_sp_addr } else { garbage_addr };
            let mut out_sp_addr = c_sp_addr;

            if !ex_accumulate && kk == 0 {
              out_sp_addr &= !(1 << (ADDR_LEN - 2));
            }

            let a_cols = (DIM - if kk == k - 1 { pad_k } else { 0 }) as u64;
            let a_rows = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
            let b_cols = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
            let b_rows = (DIM - if kk == k - 1 { pad_k } else { 0 }) as u64;
            let c_cols = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
            let c_rows = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;

            self.preload(
              (b_rows << 48) | (b_cols << 32) | pre_sp_addr as u64,
              (c_rows << 48) | (c_cols << 32) | out_sp_addr as u64,
            );

            self.compute(
              (a_rows << 48) | (a_cols << 32) | a_sp_addr as u64,
              ((DIM as u64) << 48) | ((DIM as u64) << 32) | garbage_addr as u64,
              ii == 0,
            );
          }

          // Move-out C
          if self.state.loop_ws_C != 0 && kk == k - 1 {
            let sizeof_c = if full_c {
              std::mem::size_of::<AccT>()
            } else {
              std::mem::size_of::<ElemT>()
            };
            let c_dram_addr =
              self.state.loop_ws_C + ((ii * self.state.loop_ws_C_stride as usize + jj) * DIM * sizeof_c) as u64;
            let c_cols = (DIM - if jj == j - 1 { pad_j } else { 0 }) as u64;
            let c_rows = (DIM - if ii == i - 1 { pad_i } else { 0 }) as u64;
            self.mvout(c_dram_addr, (c_rows << 48) | (c_cols << 32) | c_sp_addr as u64);
          }
        }
      }
    }
  }

  // Loop Conv WS configuration functions
  pub fn loop_conv_ws_config_1(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_conv_ws_batch_size = (rs1 & 0xFFFF) as u16;
    self.state.loop_conv_ws_in_row_dim = ((rs1 >> 16) & 0xFFFF) as u16;
    self.state.loop_conv_ws_in_channels = ((rs1 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_out_channels = ((rs1 >> 48) & 0xFFFF) as u16;

    self.state.loop_conv_ws_out_row_dim = (rs2 & 0xFFFF) as u16;
    self.state.loop_conv_ws_pool_out_row_dim = ((rs2 >> 16) & 0xFFFF) as u16;
    self.state.loop_conv_ws_out_col_dim = ((rs2 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_stride = ((rs2 >> 48) & 0xFF) as u16;
    self.state.loop_conv_ws_padding = ((rs2 >> 56) & 0xFF) as u16;
  }

  pub fn loop_conv_ws_config_2(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_conv_ws_kernel_dim = ((rs1 >> 48) & 0xFFFF) as u16;
    self.state.loop_conv_ws_pool_out_col_dim = ((rs1 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_pool_size = ((rs1 >> 16) & 0xFFFF) as u16;
    self.state.loop_conv_ws_pool_stride = ((rs1 >> 8) & 0xFF) as u16;
    self.state.loop_conv_ws_pool_padding = (rs1 & 0xFF) as u16;

    self.state.loop_conv_ws_batches = ((rs2 >> 48) & 0xFFFF) as u16;
    self.state.loop_conv_ws_porows = ((rs2 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_pocols = ((rs2 >> 16) & 0xFFFF) as u16;
    self.state.loop_conv_ws_pochs = (rs2 & 0xFFFF) as u16;
  }

  pub fn loop_conv_ws_config_3(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_conv_ws_krows = ((rs1 >> 48) & 0xFFFF) as u16;
    self.state.loop_conv_ws_kcols = ((rs1 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_kchs = ((rs1 >> 16) & 0xFFFF) as u16;
    self.state.loop_conv_ws_lpad = (rs1 & 0xFFFF) as u16;

    self.state.loop_conv_ws_rpad = ((rs2 >> 48) & 0xFFFF) as u16;
    self.state.loop_conv_ws_upad = ((rs2 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_dpad = ((rs2 >> 24) & 0xFF) as u16;
    self.state.loop_conv_ws_plpad = ((rs2 >> 16) & 0xFF) as u16;
    self.state.loop_conv_ws_in_col_dim = (rs2 & 0xFFFF) as u16;
  }

  pub fn loop_conv_ws_config_4(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_conv_ws_orows = ((rs1 >> 48) & 0xFFFF) as u16;
    self.state.loop_conv_ws_prad = ((rs1 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_pupad = ((rs1 >> 21) & 0xFF) as u16;
    self.state.loop_conv_ws_pdpad = ((rs1 >> 10) & 0xFF) as u16;
    self.state.loop_conv_ws_kernel_dilation = (rs1 & 0xFF) as u16;

    self.state.loop_conv_ws_in_stride = ((rs2 >> 48) & 0xFFFF) as u16;
    self.state.loop_conv_ws_weight_stride = ((rs2 >> 32) & 0xFFFF) as u16;
    self.state.loop_conv_ws_out_stride = ((rs2 >> 16) & 0xFFFF) as u16;
    self.state.loop_conv_ws_ocols = (rs2 & 0xFFFF) as u16;
  }

  pub fn loop_conv_ws_config_5(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_conv_ws_weights = rs1;
    self.state.loop_conv_ws_output = rs2;
  }

  pub fn loop_conv_ws_config_6(&mut self, rs1: RegT, rs2: RegT) {
    self.state.loop_conv_ws_bias = rs1;
    self.state.loop_conv_ws_input = rs2;
  }

  pub fn loop_conv_ws(&mut self, rs1: RegT, rs2: RegT) {
    let no_bias = (rs1 & 1) != 0;
    let wrot180 = ((rs1 >> 1) & 1) != 0;
    let trans_output_1203 = ((rs1 >> 2) & 1) != 0;
    let trans_weight_1203 = ((rs1 >> 3) & 1) != 0;
    let trans_weight_0132 = ((rs1 >> 4) & 1) != 0;
    let trans_input_3120 = ((rs1 >> 5) & 1) != 0;
    let dw = ((rs1 >> 6) & 1) != 0;
    let mut max_pixels_per_row = ((rs1 >> 8) & 0xFF) as u8;
    let no_pool = (rs2 & 1) != 0;
    let downsample = ((rs2 >> 1) & 1) != 0;
    let input_dilated = ((rs2 >> 2) & 1) != 0;
    let activation = ((rs2 >> 3) & 3) as u8;
    let a_spad_id = ((rs1 >> 18) & 0b11) as u8;
    let b_spad_id = ((rs1 >> 16) & 0b11) as u8;

    if max_pixels_per_row == 0 {
      max_pixels_per_row = 1;
    }

    let batch_size = self.state.loop_conv_ws_batch_size as usize;
    let in_col_dim = self.state.loop_conv_ws_in_col_dim as usize;
    let in_row_dim = self.state.loop_conv_ws_in_row_dim as usize;
    let in_channels = self.state.loop_conv_ws_in_channels as usize;
    let out_channels = self.state.loop_conv_ws_out_channels as usize;
    let in_stride = self.state.loop_conv_ws_in_stride as usize;
    let out_stride = self.state.loop_conv_ws_out_stride as usize;
    let weight_stride = self.state.loop_conv_ws_weight_stride as usize;
    let out_col_dim = self.state.loop_conv_ws_out_col_dim as usize;
    let pool_out_col_dim = self.state.loop_conv_ws_pool_out_col_dim as usize;
    let out_row_dim = self.state.loop_conv_ws_out_row_dim as usize;
    let pool_out_row_dim = self.state.loop_conv_ws_pool_out_row_dim as usize;
    let stride = self.state.loop_conv_ws_stride as usize;
    let kernel_dim = self.state.loop_conv_ws_kernel_dim as usize;
    let kernel_dilation = self.state.loop_conv_ws_kernel_dilation as usize;
    let pool_size = self.state.loop_conv_ws_pool_size as usize;
    let pool_stride = self.state.loop_conv_ws_pool_stride as usize;
    let batches = self.state.loop_conv_ws_batches as usize;
    let porows = self.state.loop_conv_ws_porows as usize;
    let pocols = self.state.loop_conv_ws_pocols as usize;
    let pochs = self.state.loop_conv_ws_pochs as usize;
    let krows = self.state.loop_conv_ws_krows as usize;
    let kcols = self.state.loop_conv_ws_kcols as usize;
    let kchs = self.state.loop_conv_ws_kchs as usize;
    let lpad = self.state.loop_conv_ws_lpad as i32;
    let rpad = self.state.loop_conv_ws_rpad as i32;
    let upad = self.state.loop_conv_ws_upad as i32;
    let dpad = self.state.loop_conv_ws_dpad as i32;
    let plpad = self.state.loop_conv_ws_plpad as i32;
    let pupad = self.state.loop_conv_ws_pupad as i32;
    let orows = self.state.loop_conv_ws_orows as usize;
    let ocols = self.state.loop_conv_ws_ocols as usize;
    let weights = self.state.loop_conv_ws_weights;
    let output = self.state.loop_conv_ws_output;
    let bias = self.state.loop_conv_ws_bias;
    let input = self.state.loop_conv_ws_input;

    let ochs = pochs;

    // Helper macros as inline functions
    let undilated = |x: i32| -> i32 {
      if input_dilated {
        (x + 1) >> 1
      } else {
        x
      }
    };

    let ds = |x: usize| -> usize {
      if downsample {
        x >> 1
      } else {
        x
      }
    };

    let us = |x: usize| -> usize {
      if downsample {
        x << 1
      } else {
        x
      }
    };

    // Calculate image dimensions
    let dilated_krows = krows + (kernel_dilation - 1) * (krows - 1);
    let dilated_kcols = kcols + (kernel_dilation - 1) * (kcols - 1);
    let irows_without_dilation = orows * stride + dilated_krows - 1;
    let icols_without_dilation = ocols * stride + dilated_kcols - 1;
    let irows_unpadded_without_dilation = (irows_without_dilation as i32 - upad - dpad) as usize;
    let icols_unpadded_without_dilation = (icols_without_dilation as i32 - lpad - rpad) as usize;
    let ichs = kchs;

    let irows_unpadded = if input_dilated {
      (irows_unpadded_without_dilation + 1) / 2
    } else {
      irows_unpadded_without_dilation
    };
    let icols_unpadded = if input_dilated {
      (icols_unpadded_without_dilation + 1) / 2
    } else {
      icols_unpadded_without_dilation
    };

    let irows = if input_dilated {
      irows_unpadded + undilated(upad) as usize + undilated(dpad) as usize
    } else {
      irows_without_dilation
    };
    let icols = if input_dilated {
      icols_unpadded + undilated(lpad) as usize + undilated(rpad) as usize
    } else {
      icols_without_dilation
    };

    let out_channels_per_bank = ochs / DIM + if ochs % DIM != 0 { 1 } else { 0 };
    let in_channels_per_bank = kchs / DIM + if kchs % DIM != 0 { 1 } else { 0 };
    let b_rows = if trans_weight_0132 {
      in_channels_per_bank * kcols * krows * ochs
    } else {
      out_channels_per_bank * kcols * krows * kchs
    };

    // Static variables simulation (using state or constants)
    let d_sp_addr_row: u32 = 0;
    let c_sp_addr_row: u32 = 0;

    let mut a_sp_addr_start: u32 = 0;
    let mut b_sp_addr_start: u32 = (BANK_NUM * BANK_ROWS / 2 - b_rows) as u32;
    let d_sp_addr_start: u32 = (1 << (ADDR_LEN - 1)) + d_sp_addr_row;
    let c_sp_addr_start: u32 = (3 << (ADDR_LEN - 2)) + c_sp_addr_row;

    if a_spad_id == 2 {
      a_sp_addr_start = (BANK_NUM * BANK_ROWS / 2) as u32;
    }
    if b_spad_id == 2 {
      b_sp_addr_start = (BANK_NUM * BANK_ROWS - b_rows) as u32;
    }

    let garbage_addr: u32 = !0;

    // Mvin bias
    if bias != 0 {
      let max_ochs_per_mvin = if ochs < MAX_BLOCK_LEN_ACC * DIM {
        ochs
      } else {
        MAX_BLOCK_LEN_ACC * DIM
      };

      self.config(
        ((Self::scale_t_to_scale_t_bits(MVIN_SCALE_IDENTITY) as u64) << 32)
          | (((batches * orows * ocols) as u64) << 16)
          | (1 << 8)
          | (2 << 3)
          | 1,
        0,
      );

      for b in 0..batches {
        for orow in 0..orows {
          for ocol in (0..ocols).step_by(DIM) {
            let i = if ocols - ocol > DIM { DIM } else { ocols - ocol };

            for och in (0..ochs).step_by(max_ochs_per_mvin) {
              let j = if ochs - och > max_ochs_per_mvin {
                max_ochs_per_mvin
              } else {
                ochs - och
              };

              let d_sp_addr = d_sp_addr_start
                + ((och / DIM) * batches * orows * ocols + b * orows * ocols + orow * ocols + ocol) as u32;

              let bias_addr = if no_bias {
                0
              } else {
                bias + (och * std::mem::size_of::<AccT>()) as u64
              };

              self.mvin(bias_addr, ((i as u64) << 48) | ((j as u64) << 32) | d_sp_addr as u64, 2);
            }
          }
        }
      }
    }

    // Mvin input
    if input != 0 {
      let mut max_chs_per_mvin = if ichs < MAX_BLOCK_LEN * DIM {
        ichs
      } else {
        MAX_BLOCK_LEN * DIM
      };
      if trans_input_3120 {
        max_chs_per_mvin = if batches < MAX_BLOCK_LEN * DIM {
          batches
        } else {
          MAX_BLOCK_LEN * DIM
        };
      }

      let dram_stride = if trans_input_3120 {
        (batch_size * std::mem::size_of::<ElemT>()) as u32
      } else {
        (in_stride * std::mem::size_of::<ElemT>()) as u32
      };

      let spad_stride = if trans_input_3120 {
        ichs * ds(irows) * ds(icols)
      } else {
        batches * ds(irows) * ds(icols)
      };

      self.config(
        ((Self::scale_t_to_scale_t_bits(MVIN_SCALE_IDENTITY) as u64) << 32)
          | ((spad_stride as u64) << 16)
          | ((max_pixels_per_row as u64) << 8)
          | (0 << 3)
          | 1,
        us(dram_stride as usize) as u64,
      );

      let b_it = if trans_input_3120 { max_chs_per_mvin } else { 1 };
      let ich_it = if trans_input_3120 { 1 } else { max_chs_per_mvin };

      for b in (0..batches).step_by(b_it) {
        let mut irow = -undilated(upad);
        while irow < irows_unpadded as i32 + undilated(dpad) {
          let irow_padded = irow + undilated(upad);

          let mut icol = -undilated(lpad);
          while icol < icols_unpadded as i32 + undilated(rpad) {
            let i = if icol >= 0 && icol < icols_unpadded as i32 {
              let diff = icols_unpadded as i32 - icol;
              if diff > us(DIM) as i32 {
                us(DIM) as i32
              } else {
                diff
              }
            } else if icol < 0 {
              if -icol > DIM as i32 {
                DIM as i32
              } else {
                -icol
              }
            } else {
              let diff = icols_unpadded as i32 + undilated(rpad) - icol;
              if diff > DIM as i32 {
                DIM as i32
              } else {
                diff
              }
            };

            let icol_padded = icol + undilated(lpad);

            for ich in (0..ichs).step_by(ich_it) {
              let k = if trans_input_3120 {
                if batches - b > max_chs_per_mvin {
                  max_chs_per_mvin
                } else {
                  batches - b
                }
              } else if ichs - ich > max_chs_per_mvin {
                max_chs_per_mvin
              } else {
                ichs - ich
              };

              let a_sp_addr = if trans_input_3120 {
                a_sp_addr_start
                  + ((b / DIM) * spad_stride
                    + ich * ds(irows) * ds(icols)
                    + ds(irow_padded as usize) * ds(icols)
                    + ds(icol_padded as usize)) as u32
              } else {
                a_sp_addr_start
                  + ((ich / DIM) * spad_stride
                    + b * ds(irows) * ds(icols)
                    + ds(irow_padded as usize) * ds(icols)
                    + ds(icol_padded as usize)) as u32
              };

              let is_zeros = irow < 0 || irow >= irows_unpadded as i32 || icol < 0 || icol >= icols_unpadded as i32;

              let in_addr = if is_zeros {
                0
              } else if trans_input_3120 {
                input
                  + (((ich * in_row_dim * in_col_dim + irow as usize * in_col_dim + icol as usize) * batch_size + b)
                    * std::mem::size_of::<ElemT>()) as u64
              } else {
                input
                  + (((b * in_row_dim * in_col_dim + irow as usize * in_col_dim + icol as usize) * in_stride + ich)
                    * std::mem::size_of::<ElemT>()) as u64
              };

              self.mvin(
                in_addr,
                ((ds(i as usize) as u64) << 48) | ((k as u64) << 32) | a_sp_addr as u64,
                0,
              );
            }

            icol += i;
          }

          irow += us(1) as i32;
        }
      }
    }

    // Mvin weights
    if weights != 0 {
      let mut max_chs_per_mvin = if ochs < MAX_BLOCK_LEN * DIM {
        ochs
      } else {
        MAX_BLOCK_LEN * DIM
      };
      if trans_weight_0132 {
        max_chs_per_mvin = if kchs < MAX_BLOCK_LEN * DIM {
          kchs
        } else {
          MAX_BLOCK_LEN * DIM
        };
      }

      let dram_stride = if dw {
        std::mem::size_of::<ElemT>()
      } else if trans_weight_1203 {
        kernel_dim * kernel_dim * out_channels * std::mem::size_of::<ElemT>()
      } else if trans_weight_0132 {
        in_channels * std::mem::size_of::<ElemT>()
      } else {
        weight_stride * std::mem::size_of::<ElemT>()
      };

      let spad_block_stride = if trans_weight_0132 {
        krows * kcols * ochs
      } else {
        krows * kcols * kchs
      };

      self.config(
        ((Self::scale_t_to_scale_t_bits(MVIN_SCALE_IDENTITY) as u64) << 32)
          | ((spad_block_stride as u64) << 16)
          | (1 << 8)
          | (1 << 3)
          | 1,
        dram_stride as u64,
      );

      let och_it = if trans_weight_0132 { DIM } else { max_chs_per_mvin };
      let kch_it = if trans_weight_0132 { max_chs_per_mvin } else { DIM };

      for och in (0..ochs).step_by(och_it) {
        for krow in 0..krows {
          for kcol in 0..kcols {
            for kch in (0..kchs).step_by(kch_it) {
              let (k, j) = if trans_weight_0132 {
                let k_val = if ochs - och > DIM { DIM } else { ochs - och };
                let j_val = if kchs - kch > max_chs_per_mvin {
                  max_chs_per_mvin
                } else {
                  kchs - kch
                };
                (k_val, j_val)
              } else {
                let k_val = if kchs - kch > DIM { DIM } else { kchs - kch };
                let j_val = if ochs - och > max_chs_per_mvin {
                  max_chs_per_mvin
                } else {
                  ochs - och
                };
                (k_val, j_val)
              };

              let b_sp_addr = if trans_weight_0132 {
                b_sp_addr_start + ((kch / DIM) * krows * kcols * ochs + krow * kcols * ochs + kcol * ochs + och) as u32
              } else {
                b_sp_addr_start + ((och / DIM) * krows * kcols * kchs + krow * kcols * kchs + kcol * kchs + kch) as u32
              };

              let w = if dw {
                weights + ((krow * kernel_dim + kcol) * std::mem::size_of::<ElemT>()) as u64
              } else if trans_weight_1203 {
                weights
                  + (((kch * kernel_dim * kernel_dim + krow * kernel_dim + kcol) * out_channels + och)
                    * std::mem::size_of::<ElemT>()) as u64
              } else if trans_weight_0132 {
                weights
                  + (((krow * kernel_dim * out_channels + kcol * out_channels + och) * in_channels + kch)
                    * std::mem::size_of::<ElemT>()) as u64
              } else {
                weights
                  + (((krow * kernel_dim * in_channels + kcol * in_channels + kch) * weight_stride + och)
                    * std::mem::size_of::<ElemT>()) as u64
              };

              self.mvin(w, ((k as u64) << 48) | ((j as u64) << 32) | b_sp_addr as u64, 1);
            }
          }
        }
      }
    }

    // Compute
    {
      let b_it = if trans_input_3120 { DIM } else { 1 };
      let ocol_it = if trans_input_3120 {
        1
      } else {
        DIM << (if input_dilated { 1 } else { 0 })
      };

      if trans_input_3120 {
        let a_stride = (irows * icols) as u16;
        let c_stride = (orows * ocols) as u16;
        self.config(((a_stride as u64) << 16) | (1 << 7) | 0, ((c_stride as u64) << 48));
      }

      for och in (0..ochs).step_by(DIM) {
        for krow in 0..krows {
          for kcol in (0..kcols).step_by(max_pixels_per_row as usize) {
            for kch in (0..kchs).step_by(DIM) {
              let mut new_weights = true;

              for b in (0..batches).step_by(b_it) {
                for orow in 0..orows {
                  if input_dilated && ((krow * kernel_dilation + orow - upad as usize) % 2 != 0) {
                    continue;
                  }

                  let mut ocol = 0;
                  while ocol < ocols {
                    if input_dilated && ((kcol * kernel_dilation + ocol - lpad as usize) % 2 != 0) {
                      ocol += 1;
                      continue;
                    }

                    let irow = undilated((orow * stride + krow * kernel_dilation) as i32) as usize;
                    let icol = undilated((ocol * stride + kcol * kernel_dilation) as i32) as usize;

                    let c_sp_addr = c_sp_addr_start
                      + ((och / DIM) * batches * orows * ocols + b * orows * ocols + orow * ocols + ocol) as u32;

                    let pixels = if kcols - kcol > max_pixels_per_row as usize {
                      max_pixels_per_row as usize
                    } else {
                      kcols - kcol
                    };

                    let i = if trans_input_3120 {
                      if batches - b > DIM {
                        DIM
                      } else {
                        batches - b
                      }
                    } else {
                      undilated(if ocols - ocol > (DIM << (if input_dilated { 1 } else { 0 })) {
                        (DIM << (if input_dilated { 1 } else { 0 })) as i32
                      } else {
                        (ocols - ocol) as i32
                      }) as usize
                    };

                    let j = if ochs - och > DIM { DIM } else { ochs - och };
                    let k = pixels * (if kchs - kch > DIM { DIM } else { kchs - kch });

                    let a_sp_addr = if trans_input_3120 {
                      a_sp_addr_start
                        + ((b / DIM) * kchs * ds(irows) * ds(icols)
                          + kch * ds(irows) * ds(icols)
                          + ds(irow) * ds(icols)
                          + ds(icol)) as u32
                    } else {
                      a_sp_addr_start
                        + ((kch / DIM) * batches * ds(irows) * ds(icols)
                          + b * ds(irows) * ds(icols)
                          + ds(irow) * ds(icols)
                          + ds(icol)) as u32
                    };

                    let krow_ = if wrot180 { krows - krow - 1 } else { krow };
                    let kcol_ = if wrot180 { kcols - kcol - 1 } else { kcol };

                    let b_sp_addr = if trans_weight_0132 {
                      b_sp_addr_start
                        + ((kch / DIM) * krows * kcols * ochs + krow_ * kcols * ochs + kcol_ * ochs + och) as u32
                    } else {
                      b_sp_addr_start
                        + ((och / DIM) * krows * kcols * kchs + krow_ * kcols * kchs + kcol_ * kchs + kch) as u32
                    };

                    let pre_sp_addr = if new_weights { b_sp_addr } else { garbage_addr };
                    let out_sp_addr = c_sp_addr;

                    self.preload(
                      ((k as u64) << 48) | ((j as u64) << 32) | pre_sp_addr as u64,
                      ((i as u64) << 48) | ((j as u64) << 32) | out_sp_addr as u64,
                    );

                    self.compute(
                      ((i as u64) << 48) | ((k as u64) << 32) | a_sp_addr as u64,
                      ((i as u64) << 48) | ((j as u64) << 32) | garbage_addr as u64,
                      new_weights,
                    );

                    ocol += ocol_it;
                    new_weights = false;
                  }
                }
              }
            }
          }
        }
      }
    }

    // Mvout results
    if output != 0 && no_pool {
      for b in 0..batches {
        for orow in 0..orows {
          for ocol in (0..ocols).step_by(DIM) {
            let i = if ocols - ocol > DIM { DIM } else { ocols - ocol };

            for och in (0..ochs).step_by(DIM) {
              let j = if ochs - och > DIM { DIM } else { ochs - och };

              let c_sp_addr = c_sp_addr_start
                + ((och / DIM) * batches * orows * ocols + b * orows * ocols + orow * ocols + ocol) as u32;

              let out = if trans_output_1203 {
                output
                  + (((orow * out_col_dim * batch_size + ocol * batch_size + b) * out_channels + och)
                    * std::mem::size_of::<ElemT>()) as u64
              } else {
                output
                  + (((b * out_row_dim * out_col_dim + orow * out_col_dim + ocol) * out_stride + och)
                    * std::mem::size_of::<ElemT>()) as u64
              };

              self.mvout(out, ((i as u64) << 48) | ((j as u64) << 32) | c_sp_addr as u64);
            }
          }
        }
      }
    } else if output != 0 && !no_pool {
      let acc_scale = self.state.acc_shift;

      self.config(
        ((ocols as u64) << 56)
          | ((orows as u64) << 48)
          | ((pocols as u64) << 40)
          | ((porows as u64) << 32)
          | ((pool_out_col_dim as u64) << 24)
          | ((plpad as u64) << 10)
          | ((pupad as u64) << 8)
          | ((pool_size as u64) << 6)
          | ((pool_stride as u64) << 4)
          | ((activation as u64) << 2)
          | 2,
        ((Self::acc_scale_t_to_acc_scale_t_bits(acc_scale) as u64) << 32)
          | (out_stride * std::mem::size_of::<ElemT>()) as u64,
      );

      for b in 0..batches {
        for poch in (0..pochs).step_by(DIM) {
          let channels = if poch + DIM >= pochs { pochs - poch } else { DIM };

          let c_sp_addr = c_sp_addr_start + ((poch / DIM) * batches * orows * ocols + b * orows * ocols) as u32;

          self.mvout(
            output
              + (((b * pool_out_row_dim * pool_out_col_dim) * out_stride + poch) * std::mem::size_of::<ElemT>()) as u64,
            ((channels as u64) << 32) | c_sp_addr as u64,
          );
        }
      }

      self.config(
        ((activation as u64) << 2) | 2,
        ((Self::acc_scale_t_to_acc_scale_t_bits(acc_scale) as u64) << 32)
          | (out_stride * std::mem::size_of::<ElemT>()) as u64,
      );
    }
  }
}
