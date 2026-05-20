# Run workload on FPGA

proc load_image {fpga_location ddr_channel image_path} {
    puts "========== Loading Image to DDR =========="
    puts "  FPGA: $fpga_location"
    puts "  Channel: $ddr_channel"
    puts "  Image: $image_path"

    # Back-door write (no -start, defaults to channel offset 0 which is CPU's 0x80000000)
    memory -write -fpga $fpga_location -channel $ddr_channel -file $image_path
    puts "Image write completed"

    # 10s
    after 10000
}

proc run_workload {cycles} {
    puts "========== Running Workload =========="
    puts "Running in loop mode: $cycles cycles per iteration"
    puts "Waiting for program exit signal..."

    # Loop until program exits (detected by Rust FFI via scu_sim_exit DPI-C call)
    # Each iteration runs $cycles clock cycles, then checks for exit flag
    set iteration 0
    set exit_flag "sim_exit.flag"

    # Clean up old exit flag if exists
    if {[file exists $exit_flag]} {
        puts "Exit flag detected, it's an error"
        exit
    }

    while {1} {
        incr iteration
        # puts "  Iteration $iteration: running $cycles cycles..."
        run $cycles rclk

        # Check if Rust side created exit flag
        if {[file exists $exit_flag]} {
            puts "Exit flag detected, stopping simulation"
            break
        }

        # Timeout after 10000 iterations (200M cycles with default 20k per iter)
        # if {$iteration >= 10000} {
        #     puts "WARNING: Reached maximum 200M cycles, this may be too long for baremetal."
        #     puts "WARNING: If you are running OS, ignore this warning."
        # }
    }

    puts "Workload completed after $iteration iterations"
}
