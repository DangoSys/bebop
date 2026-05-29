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

proc run_workload {cycles wave wave_start} {
    puts "========== Running Workload =========="
    puts "Running in loop mode: $cycles cycles per iteration"
    puts "Waiting for program exit signal..."
    puts "Waveform dump enabled: $wave"
    puts "Waveform start cycle: $wave_start"

    # Loop until program exits (detected by Rust FFI via scu_sim_exit DPI-C call)
    # Each iteration runs $cycles clock cycles, then checks for exit flag
    set iteration 0
    set total_cycles 0
    set exit_flag "sim_exit.flag"

    # Clean up old exit flag if exists
    if {[file exists $exit_flag]} {
        puts "Exit flag detected, it's an error"
        exit
    }

    while {1} {
        incr iteration
        puts "  Iteration $iteration: running $cycles cycles..."
        # run $cycles rclk

        if {$wave} {
            if {$total_cycles + $cycles <= $wave_start} {
                run $cycles rclk
                puts "Skipping waveform capture for $cycles cycles"
            } elseif {$total_cycles >= $wave_start} {
                set trace_size [expr {$cycles / 10}]
                if {$trace_size < 1} {
                    set trace_size 1
                }
                set_trace_size $trace_size rclk
                tracedb -open waveform$iteration -vcd -overwrite
                trace_signals -add *
                run $cycles rclk
                tracedb -upload
                tracedb -close
                exec vcd2fst waveform$iteration.vcd waveform$iteration.fst
            } else {
                set skip_cycles [expr {$wave_start - $total_cycles}]
                set trace_cycles [expr {$cycles - $skip_cycles}]
                if {$skip_cycles > 0} {
                    run $skip_cycles rclk
                }
                if {$trace_cycles > 0} {
                    set trace_size [expr {$trace_cycles / 10}]
                    if {$trace_size < 1} {
                        set trace_size 1
                    }
                    set_trace_size $trace_size rclk
                    tracedb -open waveform$iteration -vcd -overwrite
                    trace_signals -add *
                    run $trace_cycles rclk
                    tracedb -upload
                    tracedb -close
                    exec vcd2fst waveform$iteration.vcd waveform$iteration.fst
                }
            }
        } else {
            run $cycles rclk
        }
        set total_cycles [expr {$total_cycles + $cycles}]

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
