# Initialize FPGA and DDR4
# This script is called by run.tcl

proc init_fpga {fpga_location} {
    puts "========== Initializing FPGA =========="

    # Initialize reset sequence (similar to p2e_ddr4_backdoor example)
    # Hold reset longer to ensure CLINT and other modules are properly initialized
    puts "Initializing reset sequence..."
    # force io_sys_rstn 0
    # run 10000rclk
    # force io_sys_rstn 1
    # run 10000rclk
    force io_sys_rstn 0
    run 10000rclk
    force io_sys_rstn 1
    run 1000000rclk

    # Run a few cycles after reset to let registers initialize
    puts "Running post-reset cycles..."
    run 1000000rclk

    # Read CLINT registers after reset to check initialization
    # puts "========== Reading CLINT Registers (After Reset) =========="
    # set mtime_after_reset [get_value P2ETop.top.soc.clint_domain.clint.time_0]
    # set mtimecmp_after_reset [get_value P2ETop.top.soc.clint_domain.clint.timecmp_0]
    # set ipi_after_reset [get_value P2ETop.top.soc.clint_domain.clint.ipi_0]

    # puts "  CLINT time_0 (mtime)     = $mtime_after_reset"
    # puts "  CLINT timecmp_0          = $mtimecmp_after_reset"
    # puts "  CLINT ipi_0              = $ipi_after_reset"
    # puts "============================================================"

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

# proc reset_system {} {
#     puts "========== Resetting System =========="

#     # Reset the system before running workload
#     puts "Resetting system..."
#     force io_sys_rstn 0
#     run 10000rclk
#     force io_sys_rstn 1

#     puts "System reset complete"
# }
