# Run workload on FPGA
# This script is called by run.tcl

proc load_image {fpga_location ddr_channel image_path} {
    puts "========== Loading Image to DDR =========="
    puts "  FPGA: $fpga_location"
    puts "  Channel: $ddr_channel"
    puts "  Image: $image_path"

    memory -write -fpga $fpga_location -channel $ddr_channel -file $image_path
    puts "Image loaded successfully"

    # 15s
    after 15000
}

proc run_workload {cycles} {
    puts "========== Running Workload =========="


    # puts "Running simulation for $cycles cycles..."
    # run $cycles rclk

    # Now capture waveform for a shorter period
    # set trace_cycles 500000

    # Capture waveform using trace in VCD format
    puts "Opening trace database (VCD format)..."
    tracedb -open wave -vcd -overwrite

    # Add signals to trace (add all traced signals)
    puts "Adding signals to trace..."
    trace_signals -add *

    puts "Running simulation with trace..."

    # # Force enable timer interrupt by setting mie register
    # puts "Forcing mie register to enable timer interrupt (MSIP + MTIE)..."
    # force P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mie 'b0000000000000000000000000000000000000000000000000000000010001000

    # # Force enable global interrupts
    # puts "Forcing mstatus.mie to enable global interrupts..."
    # force P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mstatus_mie 'b1

    run $cycles rclk
    

    puts "Uploading trace data..."
    tracedb -upload

    puts "Closing trace database..."
    tracedb -close

    puts "Workload completed"
    puts "Waveform saved to wave.vcd"

    # Convert VCD to FST for smaller file size and faster loading
    puts "Converting VCD to FST format..."
    if {[catch {exec vcd2fst wave.vcd wave.fst} result]} {
        puts "Warning: vcd2fst conversion failed: $result"
    } else {
        puts "FST waveform saved to wave.fst"
        puts "You can view it with: gtkwave wave.fst"
    }
}
