# Initialize FPGA and DDR4
# This script is called by run.tcl

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

    # Initialize reset sequence (similar to p2e_ddr4_backdoor example)
    # Hold reset longer to ensure CLINT and other modules are properly initialized
    puts "Initializing reset sequence..."

    # Open waveform capture for the reset sequence to debug PC initialization
    puts "Opening reset-phase waveform capture..."
    # set_trace_size 100000 rclk
    # tracedb -open wave_reset -vcd -overwrite
    # trace_signals -add *

    force io_sys_rstn 0
    run 10000rclk
    force io_sys_rstn 1
    # run 1200000rclk

    # force 

    # puts "Closing reset-phase waveform capture..."
    # tracedb -upload
    # tracedb -close
    # exec vcd2fst wave_reset.vcd wave_reset.fst
    # puts "Reset waveform saved to wave_reset.fst"

    # # Periodic waveform capture: every 1000000 rclk, dump the first 100000 rclk
    # # Total: 100 cycles * 1000000 rclk = 100000000 rclk
    # puts "Starting periodic waveform capture (100 cycles)..."
    # for {set i 0} {$i < 10} {incr i} {
    #     puts "Periodic capture cycle $i: dumping first 100000 rclk..."
    #     tracedb -open wave_init_$i -vcd -overwrite
    #     trace_signals -add *
    #     run 100000 rclk
    #     tracedb -upload
    #     tracedb -close
    #     exec vcd2fst wave_init_$i.vcd wave_init_$i.fst
    #     puts "Periodic capture cycle $i: running remaining 900000 rclk (no trace)..."
    #     run 9900000 rclk
    # }
    # puts "Periodic waveform capture complete."



    # Poll DDR calibration status: run a chunk of cycles, then check, repeat until done
    puts "Polling DDR calibration status..."
    set calib_done "'b0"

    for {set i 0} {$i < 100000} {incr i} {
        run 100 rclk
        set calib_done [get_value io_init_calib_complete]
        puts "  Iteration $i: io_init_calib_complete = $calib_done"

        if {$calib_done != "'b0"} {
            break
        }
    }

    if {$calib_done == "'b0"} {
        puts "ERROR: DDR calibration failed after 100 iterations"
        exit 1
    }

    puts "DDR calibration complete after $i iterations!"

    # Don't run here - let CPU start executing after program is loaded
    # run 100000000 rclk

    # Read CLINT registers again after DDR calibration
    # puts "========== Reading CLINT Registers (After DDR Calibration) =========="
    # puts "Reading CLINT mtime, mtimecmp, and ipi registers..."

    # set mtime [get_value P2ETop.top.soc.clint_domain.clint.time_0]
    # set mtimecmp [get_value P2ETop.top.soc.clint_domain.clint.timecmp_0]
    # set ipi [get_value P2ETop.top.soc.clint_domain.clint.ipi_0]

    # puts "  CLINT time_0 (mtime)     = $mtime"
    # puts "  CLINT timecmp_0          = $mtimecmp"
    # puts "  CLINT ipi_0              = $ipi"
    # puts "====================================================================="

    # # Force enable timer interrupt by setting mie register
    # # mie register is in CSRFile: P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mie
    # # MTIE (Machine Timer Interrupt Enable) = bit 7 = 0x80
    # # Also need to enable global interrupts: mstatus.MIE = bit 3
    # puts "========== Forcing Timer Interrupt Enable =========="
    # puts "Setting mie register to enable timer interrupt (MTIE = bit 7)..."

    # # Set mie = 0x80 (enable MTIE) - use binary format for vdbg
    # # 0x80 = 0b10000000 (bit 7 set)
    # # 64-bit value: 0x0000000000000080 = 0b0000...010000000
    # # force P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mie 'b0000000000000000000000000000000000000000000000000000000010000000

    # # Set mstatus.mie = 1 (enable global interrupts)
    # # force P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mstatus_mie 'b1

    # puts "  mie register forced to 0x80 (MTIE enabled)"
    # puts "  mstatus.mie forced to 1 (global interrupts enabled)"
    # puts "===================================================="

    # # Trigger timer interrupt to wake CPU from WFI
    # # CLINT base address: 0x02000000
    # # mtime offset: 0xBFF8 (64-bit)
    # # mtimecmp offset: 0x4000 (64-bit, hart 0)
    # puts "Configuring CLINT to trigger timer interrupt..."

    # puts "Timer interrupt configured (mtime=0x1000, mtimecmp=0x100)"
    # puts "FPGA initialized successfully"
}

