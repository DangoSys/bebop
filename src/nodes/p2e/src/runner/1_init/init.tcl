# Initialize FPGA and DDR4

# Reset的逻辑：
# 系统io_sys_rstn=0启动reset，io_sys_rstn=1释放reset
# 对于设计soc_reset=1启动reset，soc_reset=0释放reset
# soc_reset = !io_sys_rstn || !c0_init_calib_complete
# 而ddr的reset又依赖io_sys_rstn
# 所以一个正确的初始化流程应该是io_sys_rstn=0，然后cpu和ddr都被初始化
# ddr初始化完成后c0_init_calib_complete=1，此时将io_sys_rstn=1释放ddr和CPU
# 总结：io_sys_rstn=0 -> c0_init_calib_complete=1 -> io_sys_rstn=1

proc init_fpga {fpga_location} {
    puts "========== Initializing FPGA =========="
    puts "Initializing reset sequence..."

    puts "Opening reset-phase waveform capture..."
    # set_trace_size 100000 rclk
    # tracedb -open wave_reset -vcd -overwrite
    # trace_signals -add *

    force io_sys_rstn 0
    run 100rclk
    force io_sys_rstn 1

    puts "Polling DDR calibration status..."
    set calib_done "'b0"

    for {set i 0} {$i < 100000} {incr i} {
        run 100 rclk
        set calib_done [get_value io_init_calib_complete]
        # puts "  Iteration $i: io_init_calib_complete = $calib_done"

        if {$calib_done != "'b0"} {
            break
        }
    }

    if {$calib_done == "'b0"} {
        puts "ERROR: DDR calibration failed after 100 iterations"
        exit 1
    }

    puts "DDR calibration complete after $i iterations!"
}

