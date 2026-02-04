#!/usr/bin/env python3
"""
Checkpoint management for periodic checkpointing
"""

import os
import m5


class CheckpointManager:
    """Manages periodic instruction-based checkpointing"""

    def __init__(self, system, checkpoint_base_dir='m5out/cpt'):
        self.system = system
        self.checkpoint_base_dir = checkpoint_base_dir

    def take_periodic_checkpoints(self, interval_insts):
        """Take periodic checkpoints at instruction intervals

        Args:
            interval_insts: Number of committed instructions between checkpoints

        Returns:
            Total number of checkpoints created
        """
        os.makedirs(self.checkpoint_base_dir, exist_ok=True)
        print(f"Taking checkpoints every {interval_insts} committed instructions under base dir: {self.checkpoint_base_dir}")

        checkpoint_index = 0
        next_inst_count = interval_insts

        while True:
            # Schedule instruction stop at next checkpoint point
            self.system.cpu.scheduleInstStop(0, next_inst_count, 'inst stop')

            # Run until the instruction stop event
            exit_event = m5.simulate()
            cause = exit_event.getCause()

            if cause == "inst stop":
                # Reached instruction milestone: take checkpoint
                ckpt_dir = os.path.join(self.checkpoint_base_dir, f"cpt_{checkpoint_index}")
                os.makedirs(ckpt_dir, exist_ok=True)
                print(f"Taking checkpoint #{checkpoint_index} @ {next_inst_count} instructions into: {ckpt_dir}")
                m5.checkpoint(ckpt_dir)
                checkpoint_index += 1
                next_inst_count += interval_insts
            else:
                # Workload ended or other event
                print(f"Simulation finished @ tick {m5.curTick()} because {cause}")
                break

        return checkpoint_index

    def restore_checkpoint(self, checkpoint_path):
        """Validate checkpoint path exists

        Args:
            checkpoint_path: Path to checkpoint directory

        Returns:
            True if valid, False otherwise
        """
        if not os.path.isdir(checkpoint_path):
            print(f"Error: checkpoint directory not found at {checkpoint_path}")
            return False
        print(f"Restoring from checkpoint: {checkpoint_path}")
        return True
