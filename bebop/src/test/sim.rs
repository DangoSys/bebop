use std::env;
use std::io;
use std::io::Write;

// Copy decode_funct function since frontend module is private
fn decode_funct(funct: u32) -> u8 {
  match funct {
    31 => 0,      // Fence -> domain 0
    24 | 25 => 1, // Load -> domain 1 (memdomain)
    _ => 2,       // Compute -> domain 2 (balldomain)
  }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ModelState {
  Busy,       // INFINITY - 忙碌状态
  Ready,      // 1.0 - 准备好接受新任务
  Processing, // 0.5 - 正在处理
}

pub struct Sim {
  global_time: f64,

  // Decoder state
  decoder_state: ModelState,
  decoded_inst: Option<(u32, u64, u64, u8)>,
  tmp1: Option<(u32, u64, u64, u8)>,

  // ROB state
  rob_state: ModelState,
  rob_entry: Option<(u32, u32, u64, u64, u8)>,
  tmp2: Option<(u32, u32, u64, u64, u8)>,

  // RS state
  rs_state: ModelState,
  rs_entry: Option<(u32, u32, u64, u64, u8)>,
  tmp3: Option<(u32, u32, u64, u64, u8)>,
}

impl Sim {
  pub fn new() -> Self {
    Self {
      global_time: 0.0,
      decoder_state: ModelState::Ready,
      decoded_inst: None,
      tmp1: None,
      rob_state: ModelState::Ready,
      rob_entry: None,
      tmp2: None,
      rs_state: ModelState::Ready,
      rs_entry: None,
      tmp3: None,
    }
  }

  fn model_ready(&self, state: ModelState) -> bool {
    state == ModelState::Ready
  }

  fn model_update(&self, state: ModelState) -> bool {
    state == ModelState::Processing
  }

  fn get_state_time(&self, state: ModelState) -> f64 {
    match state {
      ModelState::Busy => f64::INFINITY,
      ModelState::Ready => 1.0,
      ModelState::Processing => 0.5,
    }
  }

  pub fn inst_execute(&mut self, (funct, xs1, xs2): (Option<u32>, Option<u64>, Option<u64>)) {
    let mut raw_inst: (Option<u32>, Option<u64>, Option<u64>) = (None, None, None);
    if funct.is_some() && xs1.is_some() && xs2.is_some() {
      raw_inst = (funct, xs1, xs2);
    }

    // Decoder: ready -> processing
    if self.model_ready(self.decoder_state) && raw_inst != (None, None, None) {
      let funct = raw_inst.0.unwrap();
      let xs1 = raw_inst.1.unwrap();
      let xs2 = raw_inst.2.unwrap();
      let domain_id = decode_funct(funct);
      self.tmp1 = Some((funct, xs1, xs2, domain_id));
      self.decoder_state = ModelState::Processing;
    } else {
      self.tmp1 = None;
    }

    // ROB: ready -> processing
    if self.model_ready(self.rob_state) && self.decoded_inst.is_some() {
      let decoded = self.decoded_inst.unwrap();
      static mut ROB_COUNTER: u32 = 0;
      unsafe {
        ROB_COUNTER += 1;
        self.tmp2 = Some((ROB_COUNTER, decoded.0, decoded.1, decoded.2, decoded.3));
      }
      self.rob_state = ModelState::Processing;
    } else {
      self.tmp2 = None;
    }

    // RS: ready -> processing
    if self.model_ready(self.rs_state) && self.rob_entry.is_some() {
      self.tmp3 = self.rob_entry;
      self.rs_state = ModelState::Processing;
    } else {
      self.tmp3 = None;
    }

    // Note: Processing -> Ready updates happen in cycle_advance() at 0.5 cycle mark
  }

  pub fn cycle_advance(&mut self) -> io::Result<()> {
    let time1 = self.global_time;

    // Always advance by 1.0 cycle
    let time_delta = 1.0;

    // Update states: Processing -> Ready happens at integer cycle boundaries
    if self.model_update(self.decoder_state) {
      self.decoded_inst = self.tmp1;
      if self.decoded_inst.is_some() {
        println!("decoded_inst: {:?}", self.decoded_inst);
      }
      self.decoder_state = ModelState::Ready;
    }

    if self.model_update(self.rob_state) {
      self.rob_entry = self.tmp2;
      if self.rob_entry.is_some() {
        println!("rob_entry: {:?}", self.rob_entry);
      }
      self.rob_state = ModelState::Ready;
    }

    if self.model_update(self.rs_state) {
      self.rs_entry = self.tmp3;
      if self.rs_entry.is_some() {
        println!("rs_entry: {:?}", self.rs_entry);
      }
      self.rs_state = ModelState::Ready;
    }

    self.global_time += time_delta;

    let time2 = self.global_time;
    println!("Time: {:.1} -> {:.1} (delta: {:.1})", time1, time2, time_delta);
    Ok(())
  }

  pub fn get_global_time(&self) -> f64 {
    self.global_time
  }
}

fn wait_for_enter() -> io::Result<bool> {
  print!("Press Enter to continue, 'q' to quit: ");
  io::stdout().flush()?;
  let mut input = String::new();
  io::stdin().read_line(&mut input)?;
  Ok(input.trim() != "q")
}

fn run_step_mode(sim: &mut Sim) -> io::Result<()> {
  println!("Step mode - Press Enter to continue, 'q' to quit");
  println!("Press Enter to continue...\n");

  loop {
    if !wait_for_enter()? {
      println!("Exiting...");
      break;
    }

    println!();
    sim.inst_execute((None, None, None));
    sim.cycle_advance()?;
    println!();
  }

  Ok(())
}

fn run_continuous_mode(sim: &mut Sim) -> io::Result<()> {
  println!("Continuous mode\n");

  // Test with an instruction
  println!("=== Test instruction execution ===\n");
  sim.inst_execute((Some(24), Some(0x80003280), Some(0x2080000)));
  sim.cycle_advance()?; // 0.5 cycle

  sim.inst_execute((None, None, None));
  sim.cycle_advance()?; // 1.0 cycle

  sim.inst_execute((None, None, None));
  sim.cycle_advance()?; // 0.5 cycle

  sim.inst_execute((None, None, None));
  sim.cycle_advance()?; // 1.0 cycle

  Ok(())
}

fn main() -> std::io::Result<()> {
  let args: Vec<String> = env::args().collect();

  let step_mode = args.iter().any(|arg| arg == "--step" || arg == "-s");

  let mut sim = Sim::new();

  if step_mode {
    sim.inst_execute((Some(24), Some(0x80003280), Some(0x2080000)));
    sim.cycle_advance()?; // 0.5 cycle
    sim.inst_execute((Some(25), Some(0x80003999), Some(0x2080001)));
    sim.cycle_advance()?; // 0.5 cycle
    run_step_mode(&mut sim)?;
  } else {
    run_continuous_mode(&mut sim)?;
  }

  Ok(())
}
