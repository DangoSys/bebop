# Initialize FPGA and DDR4
# This script is called by run.tcl

proc init_fpga {fpga_location} {
    puts "========== Initializing FPGA =========="

    # Wait for initialization
    after 1000

    # Reset the system
    # Use io_sys_rstn which is declared in vcom_compile.tcl with write_net
    puts "Resetting system..."
    force io_sys_rstn 0
    run 10000rclk
    force io_sys_rstn 1

    puts "FPGA initialized successfully"
}
