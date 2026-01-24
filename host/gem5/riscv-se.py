#!/usr/bin/env python3
"""
Simple gem5 configuration script to run a hello world program on RISCV
"""

import os
import sys
import argparse
import atexit
import signal
import m5
import m5.stats
from m5.objects import *
from m5.objects import RiscvBebopInOCPU 

# Parse command line arguments
parser = argparse.ArgumentParser(description='Run a binary on RISCV using gem5')
parser.add_argument('--test-binary', required=True, help='Path to the binary to execute')
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

setInterpDir(interp_dir)
print(f"Using dynamic linker directory: {interp_dir}")

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
system.cpu.createThreads()

# Create root and instantiate
root = Root(full_system=False, system=system)
m5.instantiate()

# Run simulation
print("Beginning simulation!")
exit_event = m5.simulate()
print(f"Exiting @ tick {m5.curTick()} because {exit_event.getCause()}")
