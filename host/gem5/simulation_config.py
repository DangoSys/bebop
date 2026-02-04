#!/usr/bin/env python3
"""
Core gem5 simulation configuration for RISC-V system-call emulation
"""

import os
import sys
import shutil
from m5.objects import *
from m5.core import setInterpDir


class SimulationConfig:
    """Configures gem5 system for RISC-V binary simulation"""

    def __init__(self, test_binary):
        self.test_binary = test_binary
        self.system = None
        self.interp_dir = None

    def validate_binary(self):
        """Check if binary exists

        Returns:
            True if valid, False otherwise
        """
        if not os.path.exists(self.test_binary):
            print(f"Error: binary not found at {self.test_binary}")
            return False
        return True

    def find_riscv_toolchain_sysroot(self):
        """Find RISC-V toolchain sysroot, prioritizing conda environment

        Returns:
            Path to sysroot or None if not found
        """
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

    def setup_system(self, cpu_type='bebop', use_atomic=False):
        """Create and configure the gem5 system

        Args:
            cpu_type: Type of CPU to use ('bebop', 'atomic', 'timing', 'minor', 'o3')
            use_atomic: Force atomic memory mode (required for SimPoint)

        Returns:
            Configured system object
        """
        # Create system
        self.system = System()

        # Set up clock domain
        self.system.clk_domain = SrcClockDomain()
        self.system.clk_domain.clock = "1GHz"
        self.system.clk_domain.voltage_domain = VoltageDomain()

        # Set memory mode and range
        if use_atomic or cpu_type == 'atomic':
            self.system.mem_mode = "atomic"
        else:
            self.system.mem_mode = "timing"
        self.system.mem_ranges = [AddrRange("32GiB")]

        # Create CPU based on type
        if cpu_type == 'bebop':
            self.system.cpu = RiscvBebopInOCPU()
        elif cpu_type == 'atomic':
            self.system.cpu = RiscvAtomicSimpleCPU()
        elif cpu_type == 'timing':
            self.system.cpu = RiscvTimingSimpleCPU()
        elif cpu_type == 'minor':
            self.system.cpu = RiscvMinorCPU()
        elif cpu_type == 'o3':
            self.system.cpu = RiscvO3CPU()
        else:
            print(f"Warning: Unknown CPU type '{cpu_type}', defaulting to bebop")
            self.system.cpu = RiscvBebopInOCPU()

        # Create memory bus
        self.system.membus = SystemXBar()

        # Connect CPU to memory bus
        self.system.cpu.icache_port = self.system.membus.cpu_side_ports
        self.system.cpu.dcache_port = self.system.membus.cpu_side_ports

        # Create interrupt controller
        self.system.cpu.createInterruptController()

        # Create memory controller
        self.system.mem_ctrl = MemCtrl()
        self.system.mem_ctrl.dram = DDR3_1600_8x8()
        self.system.mem_ctrl.dram.range = self.system.mem_ranges[0]
        self.system.mem_ctrl.port = self.system.membus.mem_side_ports

        # Connect system port
        self.system.system_port = self.system.membus.cpu_side_ports

        return self.system

    def setup_workload(self):
        """Configure workload and process

        Returns:
            True on success
        """
        # Set up dynamic linker directory
        self.interp_dir = self.find_riscv_toolchain_sysroot()

        if self.interp_dir is not None:
            setInterpDir(self.interp_dir)
            print(f"Using dynamic linker directory: {self.interp_dir}")
        else:
            print("Warning: could not find RISC-V toolchain sysroot; "
                  "assuming the binary does not need a dynamic linker.")

        # Set up workload
        self.system.workload = SEWorkload.init_compatible(self.test_binary)

        # Create process
        process = Process()
        process.cmd = [self.test_binary]

        # Set up library search path for dynamic linker
        env_list = []
        if self.interp_dir:
            lib_path = os.path.join(self.interp_dir, "lib")
            if os.path.exists(lib_path):
                ld_library_path = f"LD_LIBRARY_PATH={lib_path}"
                env_list.append(ld_library_path)
                print(f"Setting LD_LIBRARY_PATH to {lib_path}")

        # Set environment variables
        if env_list:
            process.env = env_list

        self.system.cpu.workload = process
        self.system.cpu.createThreads()

        return True

    def get_system(self):
        """Get the configured system

        Returns:
            System object
        """
        return self.system

    def get_cpu(self):
        """Get the CPU object

        Returns:
            CPU object
        """
        return self.system.cpu if self.system else None
