# Initialize FPGA and DDR4
# This script is called by run.tcl

proc init_fpga {fpga_location} {
    puts "========== Initializing FPGA =========="

    # Initialize reset sequence (similar to p2e_ddr4_backdoor example)
    # Hold reset, run some cycles, then release reset
    puts "Initializing reset sequence..."
    force io_sys_rstn 0
    run 100rclk
    force io_sys_rstn 1

    # Run simulation in background to let DDR calibration complete
    puts "Starting DDR calibration (running in background)..."
    run -nowait
    after 2000
    stop

    # Check DDR calibration status
    puts "Checking DDR calibration status..."
    set calib_done [get_value io_init_calib_complete]
    puts "  io_init_calib_complete = $calib_done"

    if {$calib_done == "'b0"} {
        puts "ERROR: DDR calibration failed"
        exit 1
    }

    puts "DDR calibration complete!"
    puts "FPGA initialized successfully"
}

proc reset_system {} {
    puts "========== Resetting System =========="

    # Reset the system before running workload
    puts "Resetting system..."
    force io_sys_rstn 0
    run 10000rclk
    force io_sys_rstn 1

    puts "System reset complete"
}
