#!/usr/bin/env python3
"""
SimPoint profiling and checkpoint management
"""

import os
import sys
import re
import m5
import m5.stats


class SimPointManager:
    """Manages SimPoint profiling and checkpoint creation/restoration"""

    def __init__(self, cpu, checkpoint_base_dir='m5out/cpt'):
        self.cpu = cpu
        self.checkpoint_base_dir = checkpoint_base_dir
        self.simpoint_info = []  # List of (interval, weight, start_inst, warmup_length)
        self.simpoint_start_insts = []
        self.interval_length = None
        self.warmup_length = None

    def enable_profiling(self, interval=10000000):
        """Enable SimPoint BBV profiling

        Args:
            interval: SimPoint interval in number of instructions
        """
        self.cpu.addSimPointProbe(interval)
        print(f"SimPoint profiling enabled with interval {interval}")

    def parse_simpoint_files(self, simpoint_file, weight_file, interval_length, warmup_length):
        """Parse SimPoint and weight files to prepare checkpoint information

        Args:
            simpoint_file: Path to simpoints.txt file
            weight_file: Path to weights.txt file
            interval_length: Interval length in instructions
            warmup_length: Warmup length in instructions

        Returns:
            True on success, False on error
        """
        self.interval_length = interval_length
        self.warmup_length = warmup_length

        # Validate files exist
        if not os.path.exists(simpoint_file):
            print(f"Error: SimPoint file not found: {simpoint_file}")
            print("Hint: You need to:")
            print("  1. First run with --simpoint-profile to generate BBV file")
            print("  2. Use SimPoint 3.2 tool to analyze BBV and generate simpoints.txt and weights.txt")
            print("  3. Then run with --take-simpoint-checkpoints")
            return False

        if not os.path.exists(weight_file):
            print(f"Error: Weight file not found: {weight_file}")
            print("Hint: You need to:")
            print("  1. First run with --simpoint-profile to generate BBV file")
            print("  2. Use SimPoint 3.2 tool to analyze BBV and generate simpoints.txt and weights.txt")
            print("  3. Then run with --take-simpoint-checkpoints")
            return False

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
            return False

        # Calculate starting instruction counts
        for i, (interval, weight) in enumerate(zip(simpoints, weights)):
            if interval * interval_length - warmup_length > 0:
                starting_inst_count = interval * interval_length - warmup_length
                actual_warmup_length = warmup_length
            else:
                starting_inst_count = 0
                actual_warmup_length = interval * interval_length

            self.simpoint_info.append((interval, weight, starting_inst_count, actual_warmup_length))
            self.simpoint_start_insts.append(starting_inst_count)

        # Sort by starting instruction count
        self.simpoint_info.sort(key=lambda x: x[2])
        self.simpoint_start_insts = sorted(self.simpoint_start_insts)

        print(f"Found {len(self.simpoint_start_insts)} SimPoints")
        for i, (interval, weight, start_inst, warmup) in enumerate(self.simpoint_info):
            print(f"  SimPoint {i}: interval={interval}, weight={weight}, start_inst={start_inst}, warmup={warmup}")

        # Configure CPU with SimPoint start instructions
        self.cpu.simpoint_start_insts = self.simpoint_start_insts

        return True

    def take_simpoint_checkpoints(self):
        """Take SimPoint checkpoints based on parsed SimPoint information

        Returns:
            Number of checkpoints created
        """
        os.makedirs(self.checkpoint_base_dir, exist_ok=True)
        print(f"Taking SimPoint checkpoints under base dir: {self.checkpoint_base_dir}")

        num_checkpoints = 0
        index = 0
        last_chkpnt_inst_count = -1

        for simpoint in self.simpoint_info:
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
                    self.checkpoint_base_dir,
                    f"cpt.simpoint_{index:02d}_inst_{starting_inst_count}_weight_{weight}_interval_{self.interval_length}_warmup_{actual_warmup_length}"
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
        return num_checkpoints

    def setup_simpoint_restore(self, checkpoint_path):
        """Setup CPU for SimPoint checkpoint restoration

        Args:
            checkpoint_path: Path to checkpoint directory

        Returns:
            True on success, False on error
        """
        # Parse checkpoint directory name to get SimPoint info
        # Format: cpt.simpoint_XX_inst_XXXXX_weight_X.XXXXX_interval_XXXXX_warmup_XXXXX
        cpt_name = os.path.basename(checkpoint_path.rstrip('/'))
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
            self.cpu.simpoint_start_insts = [warmup_length, warmup_length + interval_length]
            return True
        else:
            print("Warning: Could not parse SimPoint checkpoint name, assuming standard format")
            return False

    def run_simpoint_region(self):
        """Run a SimPoint region after restoration (warmup + measurement)

        Returns:
            Exit code
        """
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
                return exit_event.getCode()
            else:
                print(f"Unexpected exit cause after warmup: {exit_cause}")
                return 1
        else:
            print(f"Unexpected exit cause: {exit_cause}")
            return 1
