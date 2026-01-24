#!/usr/bin/env python3
"""
Simple gem5 configuration script to run a hello world program on RISCV
"""

import os
import sys
import argparse
import atexit
import signal
import re
import m5
import m5.stats
from m5.objects import *
from m5.objects import RiscvBebopInOCPU 

# Parse command line arguments
parser = argparse.ArgumentParser(description='Run a binary on RISCV using gem5')
parser.add_argument('--test-binary', required=True, help='Path to the binary to execute')
parser.add_argument(
  '--checkpoint-dir',
  default='m5out/cpt',
  help='Base directory to store or load checkpoints (default: m5out/cpt)',
)
parser.add_argument(
  '--checkpoint-interval-insts',
  type=int,
  default=None,
  help='Take periodic checkpoints every this many committed instructions (default: disabled)',
)
parser.add_argument(
  '--restore-from',
  default=None,
  help='If set, restore simulation state from this checkpoint directory',
)
parser.add_argument(
  '--simpoint-profile',
  action='store_true',
  help='Enable SimPoint BBV profiling (requires AtomicSimpleCPU)',
)
parser.add_argument(
  '--simpoint-interval',
  type=int,
  default=10000000,
  help='SimPoint interval in number of instructions (default: 10000000)',
)
parser.add_argument(
  '--take-simpoint-checkpoints',
  type=str,
  default=None,
  help='Take SimPoint checkpoints: <simpoint_file,weight_file,interval_length,warmup_length>',
)
parser.add_argument(
  '--restore-simpoint-checkpoint',
  action='store_true',
  help='Restore from a SimPoint checkpoint and run only the SimPoint region (requires --restore-from). '
       'If not set, checkpoint will be restored normally and run to completion.',
)
args = parser.parse_args()

test_binary = args.test_binary

# Check if binary exists
if not os.path.exists(test_binary):
  print(f"Error: binary not found at {test_binary}")
  sys.exit(1)

# Create system
system = System()

# Set up clock domain
system.clk_domain = SrcClockDomain()
system.clk_domain.clock = "1GHz"
system.clk_domain.voltage_domain = VoltageDomain()

# Set memory mode and range
# system.mem_mode = "atomic"
system.mem_mode = "timing"
system.mem_ranges = [AddrRange("32GiB")]

# Create CPU
# SimPoint only works with AtomicSimpleCPU
if args.simpoint_profile or args.take_simpoint_checkpoints or args.restore_simpoint_checkpoint:
  system.mem_mode = "atomic"  # SimPoint requires atomic mode
  system.cpu = RiscvAtomicSimpleCPU()
else:
  # system.cpu = AtomicSimpleCPU()
  system.cpu = RiscvBebopInOCPU()
  # system.cpu = RiscvTimingSimpleCPU()
  # system.cpu = RiscvMinorCPU()
  # system.cpu = RiscvO3CPU()

# Create memory bus
system.membus = SystemXBar()

# Connect CPU to memory bus
system.cpu.icache_port = system.membus.cpu_side_ports
system.cpu.dcache_port = system.membus.cpu_side_ports

# Create interrupt controller
system.cpu.createInterruptController()

# Create memory controller
system.mem_ctrl = MemCtrl()
system.mem_ctrl.dram = DDR3_1600_8x8()
system.mem_ctrl.dram.range = system.mem_ranges[0]
system.mem_ctrl.port = system.membus.mem_side_ports

# Connect system port
system.system_port = system.membus.cpu_side_ports

# Set up dynamic linker directory
# gem5 needs to know where to find the RISC-V dynamic linker
from m5.core import setInterpDir
import shutil

def find_riscv_toolchain_sysroot():
  """Find RISC-V toolchain sysroot, prioritizing conda environment"""
  # Try to find toolchain binary first
  toolchain_names = [
    "riscv64-unknown-linux-gnu-g++",
    "riscv64-unknown-linux-gnu-gcc",
    "riscv64-linux-gnu-g++",
    "riscv64-linux-gnu-gcc",
  ]
  
  toolchain_path = None
  for name in toolchain_names:
    path = shutil.which(name)
    if path:
      toolchain_path = path
      break
  
  # If found, try to derive sysroot from toolchain path
  if toolchain_path:
    # Common patterns: toolchain_dir/sysroot or toolchain_dir/../sysroot
    toolchain_dir = os.path.dirname(toolchain_path)
    possible_sysroots = [
      os.path.join(toolchain_dir, "sysroot"),
      os.path.join(os.path.dirname(toolchain_dir), "sysroot"),
      os.path.join(toolchain_dir, "..", "sysroot"),
    ]
    for sysroot in possible_sysroots:
      sysroot = os.path.abspath(sysroot)
      ld_path = os.path.join(sysroot, "lib/ld-linux-riscv64-lp64d.so.1")
      if os.path.exists(ld_path):
        return sysroot
  
  return None

# Priority: conda environment toolchain > system toolchain > standard locations
interp_dir = find_riscv_toolchain_sysroot()

if interp_dir is not None:
  setInterpDir(interp_dir)
  print(f"Using dynamic linker directory: {interp_dir}")
else:
  print("Warning: could not find RISC-V toolchain sysroot; "
        "assuming the binary does not need a dynamic linker.")

# Set up workload
system.workload = SEWorkload.init_compatible(test_binary)

# Create process
process = Process()
process.cmd = [test_binary]

# Set up library search path for dynamic linker
# Add sysroot/lib to LD_LIBRARY_PATH so shared libraries can be found
env_list = []
if interp_dir:
  lib_path = os.path.join(interp_dir, "lib")
  if os.path.exists(lib_path):
    ld_library_path = f"LD_LIBRARY_PATH={lib_path}"
    env_list.append(ld_library_path)
    print(f"Setting LD_LIBRARY_PATH to {lib_path}")

# Set environment variables
if env_list:
  process.env = env_list

system.cpu.workload = process

# Set up SimPoint probe for BBV profiling
if args.simpoint_profile:
  system.cpu.addSimPointProbe(args.simpoint_interval)
  print(f"SimPoint profiling enabled with interval {args.simpoint_interval}")

system.cpu.createThreads()

# Parse SimPoint checkpoint files if needed
simpoint_start_insts = []
simpoint_info = []  # List of (interval, weight, start_inst, warmup_length)
interval_length = None
warmup_length = None

if args.take_simpoint_checkpoints:
  # Parse: simpoint_file,weight_file,interval_length,warmup_length
  parts = args.take_simpoint_checkpoints.split(',')
  if len(parts) != 4:
    print("Error: --take-simpoint-checkpoints format: <simpoint_file,weight_file,interval_length,warmup_length>")
    sys.exit(1)
  
  simpoint_file, weight_file, interval_length, warmup_length = parts
  interval_length = int(interval_length)
  warmup_length = int(warmup_length)
  
  if not os.path.exists(simpoint_file):
    print(f"Error: SimPoint file not found: {simpoint_file}")
    print("Hint: You need to:")
    print("  1. First run with --simpoint-profile to generate BBV file")
    print("  2. Use SimPoint 3.2 tool to analyze BBV and generate simpoints.txt and weights.txt")
    print("  3. Then run with --take-simpoint-checkpoints")
    sys.exit(1)
  if not os.path.exists(weight_file):
    print(f"Error: Weight file not found: {weight_file}")
    print("Hint: You need to:")
    print("  1. First run with --simpoint-profile to generate BBV file")
    print("  2. Use SimPoint 3.2 tool to analyze BBV and generate simpoints.txt and weights.txt")
    print("  3. Then run with --take-simpoint-checkpoints")
    sys.exit(1)
  
  # Read SimPoint files
  simpoints = []
  with open(simpoint_file, 'r') as f:
    for line in f:
      m = re.match(r'(\d+)\s+(\d+)', line)
      if m:
        interval = int(m.group(1))
        simpoints.append(interval)
  
  weights = []
  with open(weight_file, 'r') as f:
    for line in f:
      m = re.match(r'([0-9\.e\-]+)\s+(\d+)', line)
      if m:
        weight = float(m.group(1))
        weights.append(weight)
  
  if len(simpoints) != len(weights):
    print(f"Error: SimPoint file and weight file have different number of entries")
    sys.exit(1)
  
  # Calculate starting instruction counts
  for i, (interval, weight) in enumerate(zip(simpoints, weights)):
    if interval * interval_length - warmup_length > 0:
      starting_inst_count = interval * interval_length - warmup_length
      actual_warmup_length = warmup_length
    else:
      starting_inst_count = 0
      actual_warmup_length = interval * interval_length
    
    simpoint_info.append((interval, weight, starting_inst_count, actual_warmup_length))
    simpoint_start_insts.append(starting_inst_count)
  
  # Sort by starting instruction count
  simpoint_info.sort(key=lambda x: x[2])
  simpoint_start_insts = sorted(simpoint_start_insts)
  
  print(f"Found {len(simpoint_start_insts)} SimPoints")
  for i, (interval, weight, start_inst, warmup) in enumerate(simpoint_info):
    print(f"  SimPoint {i}: interval={interval}, weight={weight}, start_inst={start_inst}, warmup={warmup}")
  
  system.cpu.simpoint_start_insts = simpoint_start_insts

# Set up SimPoint restore
if args.restore_simpoint_checkpoint:
  if not args.restore_from:
    print("Error: --restore-simpoint-checkpoint requires --restore-from")
    sys.exit(1)
  # Parse checkpoint directory name to get SimPoint info
  # Format: cpt.simpoint_XX_inst_XXXXX_weight_X.XXXXX_interval_XXXXX_warmup_XXXXX
  cpt_name = os.path.basename(args.restore_from.rstrip('/'))
  match = re.match(
    r'cpt\.simpoint_(\d+)_inst_(\d+)_weight_([\d\.e\-]+)_interval_(\d+)_warmup_(\d+)',
    cpt_name
  )
  if match:
    index = int(match.group(1))
    start_inst = int(match.group(2))
    weight = float(match.group(3))
    interval_length = int(match.group(4))
    warmup_length = int(match.group(5))
    print(f"Restoring SimPoint #{index}: start_inst={start_inst}, weight={weight}, "
          f"interval={interval_length}, warmup={warmup_length}")
    system.cpu.simpoint_start_insts = [warmup_length, warmup_length + interval_length]
  else:
    print("Warning: Could not parse SimPoint checkpoint name, assuming standard format")

# Create root and instantiate (optionally from checkpoint)
root = Root(full_system=False, system=system)

if args.restore_from:
  if not os.path.isdir(args.restore_from):
    print(f"Error: checkpoint directory not found at {args.restore_from}")
    sys.exit(1)
  print(f"Restoring from checkpoint: {args.restore_from}")
  m5.instantiate(args.restore_from)
else:
  m5.instantiate()

if args.take_simpoint_checkpoints:
  # Take SimPoint checkpoints
  os.makedirs(args.checkpoint_dir, exist_ok=True)
  print(f"Taking SimPoint checkpoints under base dir: {args.checkpoint_dir}")
  
  num_checkpoints = 0
  index = 0
  last_chkpnt_inst_count = -1
  
  for simpoint in simpoint_info:
    interval, weight, starting_inst_count, actual_warmup_length = simpoint
    
    if starting_inst_count == last_chkpnt_inst_count:
      # Same starting point as last checkpoint (warmup longer than starting point)
      exit_cause = "simpoint starting point found"
      code = 0
    else:
      exit_event = m5.simulate()
      
      # Skip checkpoint instructions if they exist
      while exit_event.getCause() == "checkpoint":
        print("Found 'checkpoint' exit event...ignoring...")
        exit_event = m5.simulate()
      
      exit_cause = exit_event.getCause()
      code = exit_event.getCode()
    
    if exit_cause == "simpoint starting point found":
      ckpt_dir = os.path.join(
        args.checkpoint_dir,
        f"cpt.simpoint_{index:02d}_inst_{starting_inst_count}_weight_{weight}_interval_{interval_length}_warmup_{actual_warmup_length}"
      )
      os.makedirs(ckpt_dir, exist_ok=True)
      print(f"Checkpoint #{index} written. start inst:{starting_inst_count} weight:{weight}")
      m5.checkpoint(ckpt_dir)
      num_checkpoints += 1
      last_chkpnt_inst_count = starting_inst_count
      index += 1
    else:
      print(f"Unexpected exit cause: {exit_cause}")
      break
  
  print(f"Total {num_checkpoints} SimPoint checkpoints created")

elif args.restore_simpoint_checkpoint:
  # Restore and run SimPoint region
  print("Running SimPoint region...")
  
  exit_event = m5.simulate()
  exit_cause = exit_event.getCause()
  
  if exit_cause == "simpoint starting point found":
    print("Warmed up! Dumping and resetting stats!")
    m5.stats.dump()
    m5.stats.reset()
    
    exit_event = m5.simulate()
    exit_cause = exit_event.getCause()
    
    if exit_cause == "simpoint starting point found":
      print("Done running SimPoint!")
      m5.stats.dump()
      sys.exit(exit_event.getCode())
    else:
      print(f"Unexpected exit cause after warmup: {exit_cause}")
  else:
    print(f"Unexpected exit cause: {exit_cause}")

elif args.checkpoint_interval_insts is not None:
  # Periodic checkpoint mode based on instruction count
  # Use scheduleInstStop to stop at specific instruction counts
  os.makedirs(args.checkpoint_dir, exist_ok=True)
  print(f"Taking checkpoints every {args.checkpoint_interval_insts} committed instructions under base dir: {args.checkpoint_dir}")

  checkpoint_index = 0
  next_inst_count = args.checkpoint_interval_insts

  while True:
    # Schedule instruction stop at next checkpoint point
    system.cpu.scheduleInstStop(0, next_inst_count, 'inst stop')
    
    # Run until the instruction stop event
    exit_event = m5.simulate()
    cause = exit_event.getCause()

    if cause == "inst stop":
      # Reached instruction milestone: take checkpoint
      ckpt_dir = os.path.join(args.checkpoint_dir, f"cpt_{checkpoint_index}")
      os.makedirs(ckpt_dir, exist_ok=True)
      print(f"Taking checkpoint #{checkpoint_index} @ {next_inst_count} instructions into: {ckpt_dir}")
      m5.checkpoint(ckpt_dir)
      checkpoint_index += 1
      next_inst_count += args.checkpoint_interval_insts
    else:
      # Workload ended or other event
      print(f"Simulation finished @ tick {m5.curTick()} because {cause}")
      break
else:
  # Normal run until workload结束
  print("Beginning simulation!")
  exit_event = m5.simulate()
  print(f"Exiting @ tick {m5.curTick()} because {exit_event.getCause()}")
