#!/usr/bin/env python3
"""
Simple gem5 configuration script to run a hello world program on RISCV
"""

import os
import sys
import argparse
import m5
from m5.objects import *

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
system.mem_mode = "timing"
system.mem_ranges = [AddrRange("2GiB")]

# Create CPU
system.cpu = RiscvTimingSimpleCPU()

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

# Set up workload
system.workload = SEWorkload.init_compatible(test_binary)

# Create process
process = Process()
process.cmd = [test_binary]
system.cpu.workload = process
system.cpu.createThreads()

# Create root and instantiate
root = Root(full_system=False, system=system)
m5.instantiate()

# Run simulation
print("Beginning simulation!")
exit_event = m5.simulate()
print(f"Exiting @ tick {m5.curTick()} because {exit_event.getCause()}")
