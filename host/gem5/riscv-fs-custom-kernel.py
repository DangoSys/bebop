#!/usr/bin/env python3
"""
RISC-V Full System simulation with custom kernel.
This allows you to use your own compiled kernel and disk image.
"""

import os
import sys
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

# ============================================================================
# Configure your custom kernel and disk image paths here
# ============================================================================
# Path to your compiled RISC-V kernel (vmlinux or bbl with embedded kernel)
# Option 1: Use interactive-bin (bbl + kernel) - RECOMMENDED
CUSTOM_KERNEL_PATH = "/home/mio/Code/buddy-examples/thirdparty/chipyard/software/firemarshal/images/firechip/interactive/interactive-bin"

# Option 2: Use plain vmlinux
# CUSTOM_KERNEL_PATH = "/home/mio/Code/buddy-examples/thirdparty/chipyard/software/firemarshal/boards/default/linux/vmlinux"

# Option 3: Use OpenSBI firmware payload
# CUSTOM_KERNEL_PATH = "/home/mio/Code/buddy-examples/thirdparty/chipyard/software/firemarshal/boards/default/firmware/opensbi/build/platform/generic/firmware/fw_payload.elf"

# Path to your disk image
CUSTOM_DISK_IMAGE_PATH = "/home/mio/Code/buddy-examples/thirdparty/chipyard/software/firemarshal/images/firechip/interactive/interactive.img"

# Kernel command line arguments (optional)
KERNEL_CMDLINE = "console=ttyS0 root=/dev/vda rw"
# ============================================================================

# Validate kernel path
if not os.path.exists(CUSTOM_KERNEL_PATH):
    print(f"Error: Kernel not found at {CUSTOM_KERNEL_PATH}")
    print("\nPlease update CUSTOM_KERNEL_PATH in this script to point to your kernel.")
    print("You can use:")
    print("  - A vmlinux kernel")
    print("  - A Berkeley Boot Loader (bbl) with embedded kernel")
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
if CUSTOM_DISK_IMAGE_PATH and os.path.exists(CUSTOM_DISK_IMAGE_PATH):
    # With disk image
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
else:
    # Without disk image (bootloader only mode)
    board.set_kernel_disk_workload(
        kernel=kernel,
        disk_image=None,
        bootloader=None,
    )
    print(f"Using custom kernel: {CUSTOM_KERNEL_PATH}")
    print("No disk image specified (bootloader-only mode)")

simulator = Simulator(board=board)
print("\nBeginning RISC-V Full System simulation with custom kernel!")
print("You can access the terminal using m5term:")
print("  ./util/term/m5term localhost <port>")
print("Look for 'Listening for connections on port <port>' in the output.")
simulator.run()
