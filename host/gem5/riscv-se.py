#!/usr/bin/env python3
"""
Top-level manager for gem5 RISC-V system-call emulation simulation

This script orchestrates simulation configuration, checkpoint management,
and SimPoint-based sampling for RISC-V binaries in gem5.
"""

import os
import sys
import argparse

# Add current directory to Python path to find our modules
current_dir = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, current_dir)

import m5
from m5.objects import Root

from simulation_config import SimulationConfig
from checkpoint_manager import CheckpointManager
from simpoint_manager import SimPointManager


def parse_arguments():
    """Parse command line arguments

    Returns:
        Parsed argument namespace
    """
    parser = argparse.ArgumentParser(
        description='Run a RISC-V binary in gem5 with optional checkpointing and SimPoint support',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Basic simulation
  %(prog)s --test-binary /path/to/binary

  # SimPoint profiling (step 1)
  %(prog)s --test-binary /path/to/binary --simpoint-profile

  # Take SimPoint checkpoints (step 2, after running SimPoint tool)
  %(prog)s --test-binary /path/to/binary --take-simpoint-checkpoints simpoints.txt,weights.txt,10000000,1000000

  # Run from SimPoint checkpoint
  %(prog)s --test-binary /path/to/binary --restore-from m5out/cpt/cpt.simpoint_00_... --restore-simpoint-checkpoint

  # Periodic checkpointing
  %(prog)s --test-binary /path/to/binary --checkpoint-interval-insts 1000000

  # Restore from checkpoint and continue
  %(prog)s --test-binary /path/to/binary --restore-from m5out/cpt/cpt_0
"""
    )

    # Basic configuration
    parser.add_argument(
        '--test-binary',
        required=True,
        help='Path to the RISC-V binary to execute'
    )

    # Checkpoint options
    checkpoint_group = parser.add_argument_group('checkpoint options')
    checkpoint_group.add_argument(
        '--checkpoint-dir',
        default='m5out/cpt',
        help='Base directory to store or load checkpoints (default: m5out/cpt)',
    )
    checkpoint_group.add_argument(
        '--checkpoint-interval-insts',
        type=int,
        default=None,
        help='Take periodic checkpoints every N committed instructions',
    )
    checkpoint_group.add_argument(
        '--restore-from',
        default=None,
        help='Restore simulation state from this checkpoint directory',
    )

    # SimPoint options
    simpoint_group = parser.add_argument_group('SimPoint options')
    simpoint_group.add_argument(
        '--simpoint-profile',
        action='store_true',
        help='Enable SimPoint BBV profiling (requires AtomicSimpleCPU)',
    )
    simpoint_group.add_argument(
        '--simpoint-interval',
        type=int,
        default=10000000,
        help='SimPoint interval in number of instructions (default: 10000000)',
    )
    simpoint_group.add_argument(
        '--take-simpoint-checkpoints',
        type=str,
        default=None,
        metavar='SIMPOINT_FILE,WEIGHT_FILE,INTERVAL,WARMUP',
        help='Take SimPoint checkpoints using: <simpoint_file,weight_file,interval_length,warmup_length>',
    )
    simpoint_group.add_argument(
        '--restore-simpoint-checkpoint',
        action='store_true',
        help='Restore from a SimPoint checkpoint and run only the SimPoint region (requires --restore-from)',
    )

    return parser.parse_args()


def determine_cpu_type(args):
    """Determine CPU type based on arguments

    Args:
        args: Parsed command line arguments

    Returns:
        CPU type string and whether to use atomic mode
    """
    # SimPoint requires atomic CPU
    if args.simpoint_profile or args.take_simpoint_checkpoints or args.restore_simpoint_checkpoint:
        return 'atomic', True
    else:
        return 'bebop', False


def run_simulation(system, args):
    """Run the main simulation based on mode

    Args:
        system: Configured gem5 system
        args: Parsed command line arguments
    """
    # Create checkpoint and SimPoint managers
    checkpoint_mgr = CheckpointManager(system, args.checkpoint_dir)
    simpoint_mgr = SimPointManager(system.cpu, args.checkpoint_dir)

    # Handle SimPoint checkpoint taking
    if args.take_simpoint_checkpoints:
        parts = args.take_simpoint_checkpoints.split(',')
        if len(parts) != 4:
            print("Error: --take-simpoint-checkpoints format: <simpoint_file,weight_file,interval_length,warmup_length>")
            sys.exit(1)

        simpoint_file, weight_file, interval_length, warmup_length = parts
        interval_length = int(interval_length)
        warmup_length = int(warmup_length)

        if not simpoint_mgr.parse_simpoint_files(simpoint_file, weight_file, interval_length, warmup_length):
            sys.exit(1)

        simpoint_mgr.take_simpoint_checkpoints()
        return

    # Handle SimPoint checkpoint restoration
    if args.restore_simpoint_checkpoint:
        if not args.restore_from:
            print("Error: --restore-simpoint-checkpoint requires --restore-from")
            sys.exit(1)

        simpoint_mgr.setup_simpoint_restore(args.restore_from)
        exit_code = simpoint_mgr.run_simpoint_region()
        sys.exit(exit_code)

    # Handle periodic checkpointing
    if args.checkpoint_interval_insts is not None:
        checkpoint_mgr.take_periodic_checkpoints(args.checkpoint_interval_insts)
        return

    # Normal simulation run
    print("Beginning simulation!")
    exit_event = m5.simulate()
    print(f"Exiting @ tick {m5.curTick()} because {exit_event.getCause()}")


def main():
    """Main entry point"""
    # Parse command line arguments
    args = parse_arguments()

    # Create simulation configuration
    sim_config = SimulationConfig(args.test_binary)

    # Validate binary exists
    if not sim_config.validate_binary():
        sys.exit(1)

    # Determine CPU type
    cpu_type, use_atomic = determine_cpu_type(args)

    # Setup system
    system = sim_config.setup_system(cpu_type=cpu_type, use_atomic=use_atomic)

    # Setup workload
    sim_config.setup_workload()

    # Enable SimPoint profiling if requested
    if args.simpoint_profile:
        simpoint_mgr = SimPointManager(sim_config.get_cpu())
        simpoint_mgr.enable_profiling(args.simpoint_interval)

    # Create root and instantiate
    root = Root(full_system=False, system=system)

    # Handle checkpoint restoration
    if args.restore_from:
        checkpoint_mgr = CheckpointManager(system, args.checkpoint_dir)
        if not checkpoint_mgr.restore_checkpoint(args.restore_from):
            sys.exit(1)
        m5.instantiate(args.restore_from)
    else:
        m5.instantiate()

    # Run simulation
    run_simulation(system, args)


# gem5 scripts don't use if __name__ == "__main__" guard
# Execute directly at module level
main()
