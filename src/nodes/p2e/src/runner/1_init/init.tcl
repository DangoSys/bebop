# Calibrate DDR while keeping the complete SoC, including caches, in reset.
# SoC reset = !io_sys_rstn || !calibration || io_soc_hold; the DDR macro only sees io_sys_rstn.
proc init_fpga {fpga_location} {
    puts "========== Initializing FPGA with SoC held in reset =========="
    force io_soc_hold 1
    force io_sys_rstn 0
    run 100rclk
    force io_sys_rstn 1

    for {set i 0} {$i < 100000} {incr i} {
        run 100 rclk
        if {[get_value io_soc_hold] ne "'b1"} {
            error "SoC hold was released before DDR images were loaded"
        }
        if {[get_value io_init_calib_complete] eq "'b1"} {
            puts "DDR calibration complete; SoC remains held in reset"
            return
        }
    }
    error "DDR calibration failed after 100000 iterations"
}

# The caller must finish every manifest image write before calling this once.
proc release_soc {} {
    if {[get_value io_init_calib_complete] ne "'b1" || [get_value io_soc_hold] ne "'b1"} {
        error "Cold-load release requires calibrated DDR and an asserted SoC hold"
    }
    force io_soc_hold 0
    run 100 rclk
    if {[get_value io_soc_hold] ne "'b0"} {
        error "SoC hold did not release after cold loading"
    }
    puts "Cold-loaded SoC released"
}
