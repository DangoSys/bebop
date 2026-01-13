#!/usr/bin/env python3
"""
RISC-V Full System simulation with custom kernel.
This allows you to use your own compiled kernel and disk image.
"""

import os
import sys
import argparse
from gem5.components.boards.riscv_board import RiscvBoard
from gem5.components.cachehierarchies.classic.private_l1_private_l2_walk_cache_hierarchy import (
    PrivateL1PrivateL2WalkCacheHierarchy,
)
from gem5.components.memory import SingleChannelDDR3_1600
from gem5.components.processors.cpu_types import CPUTypes
from gem5.components.processors.simple_processor import SimpleProcessor
from gem5.isas import ISA
from gem5.resources.resource import KernelResource, DiskImageResource
from gem5.simulate.simulator import Simulator
from gem5.utils.requires import requires

# Ensure RISC-V ISA is being used
requires(isa_required=ISA.RISCV)

# Parse command line arguments
parser = argparse.ArgumentParser(description='Run RISC-V Full System simulation with custom kernel')
parser.add_argument('--custom-kernel', required=True, help='Path to the custom kernel (vmlinux, bbl, or OpenSBI firmware)')
parser.add_argument('--custom-disk-image', required=True, help='Path to the custom disk image')
args = parser.parse_args()

CUSTOM_KERNEL_PATH = args.custom_kernel
CUSTOM_DISK_IMAGE_PATH = args.custom_disk_image

# Kernel command line arguments (optional)
KERNEL_CMDLINE = "console=ttyS0 root=/dev/vda rw"

# Validate kernel path
if not os.path.exists(CUSTOM_KERNEL_PATH):
    print(f"Error: Kernel not found at {CUSTOM_KERNEL_PATH}")
    sys.exit(1)

# Validate disk image path
if not os.path.exists(CUSTOM_DISK_IMAGE_PATH):
    print(f"Error: Disk image not found at {CUSTOM_DISK_IMAGE_PATH}")
    sys.exit(1)

# Setup cache hierarchy
cache_hierarchy = PrivateL1PrivateL2WalkCacheHierarchy(
    l1d_size="32KiB",
    l1i_size="32KiB",
    l2_size="512KiB"
)

# Setup memory
memory = SingleChannelDDR3_1600(size="2GiB")

# Setup processor
processor = SimpleProcessor(
    cpu_type=CPUTypes.ATOMIC,
    isa=ISA.RISCV,
    num_cores=1
)

# Setup the board
board = RiscvBoard(
    clk_freq="1GHz",
    processor=processor,
    memory=memory,
    cache_hierarchy=cache_hierarchy,
)

# Create kernel resource from your custom kernel
kernel = KernelResource(
    local_path=CUSTOM_KERNEL_PATH,
    root_partition=None,
)

# Set Full System workload with custom kernel
disk_image = DiskImageResource(
    local_path=CUSTOM_DISK_IMAGE_PATH,
    root_partition="1",  # Adjust if your root partition is different
)
board.set_kernel_disk_workload(
    kernel=kernel,
    disk_image=disk_image,
    bootloader=None,
    readfile_contents=None,
    kernel_args=[KERNEL_CMDLINE] if KERNEL_CMDLINE else [],
)
print(f"Using custom kernel: {CUSTOM_KERNEL_PATH}")
print(f"Using custom disk image: {CUSTOM_DISK_IMAGE_PATH}")

simulator = Simulator(board=board)
print("\nBeginning RISC-V Full System simulation with custom kernel!")
print("You can access the terminal using m5term:")
print("  ./util/term/m5term localhost <port>")
print("Look for 'Listening for connections on port <port>' in the output.")
simulator.run()
