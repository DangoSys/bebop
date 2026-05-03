# Initialize FPGA and DDR4
# This script is called by run.tcl

proc init_fpga {fpga_location} {
    puts "========== Initializing FPGA =========="

    # Wait for initialization
    after 1000

    # Check DDR calibration complete
    puts "Checking DDR calibration..."
    set calib_done [get_value io_init_calib_complete]
    if {$calib_done != 1} {
        puts "ERROR: DDR calibration not complete (io_init_calib_complete = $calib_done)"
        exit 1
    }
    puts "DDR calibration complete"

    puts "FPGA initialized successfully"
}

proc reset_system {} {
    puts "========== Resetting System =========="

    # Reset the system
    # Use io_sys_rstn which is declared in vcom_compile.tcl with write_net
    puts "Resetting system..."
    force io_sys_rstn 0
    run 10000rclk
    force io_sys_rstn 1

    puts "System reset complete"
}
