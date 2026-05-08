# Run workload on FPGA
# This script is called by run.tcl

proc load_image {fpga_location ddr_channel image_path} {
    puts "========== Loading Image to DDR =========="
    puts "  FPGA: $fpga_location"
    puts "  Channel: $ddr_channel"
    puts "  Image: $image_path"

    # Back-door write (no -start, defaults to channel offset 0 which is CPU's 0x80000000)
    memory -write -fpga $fpga_location -channel $ddr_channel -file $image_path
    puts "Image write completed"

    # Back-door read to verify (also from channel offset 0)
    puts "========== Verifying DDR Content =========="
    set readback_file "[file dirname $image_path]/ddr_readback.hex"
    memory -read -fpga $fpga_location -channel $ddr_channel -file $readback_file -start 0 -end 255
    puts "Read back to: $readback_file"

    # Print first 32 bytes for quick verification
    puts "First 32 bytes from DDR (channel offset 0 = CPU's 0x80000000):"
    set fp [open $readback_file r]
    set line_count 0
    while {[gets $fp line] >= 0 && $line_count < 33} {
        puts "  $line"
        incr line_count
    }
    close $fp

    # 15s
    after 15000
}

proc run_workload {cycles} {
    puts "========== Running Workload =========="

    # Before running, read DDR at address 0x80000000 to see what's there
    puts "========== Reading DDR at 0x80000000 =========="
    set readback_0x80000000 "/tmp/ddr_readback_0x80000000.hex"

    # Try reading from physical address 0x80000000 (where CPU is trying to access)
    puts "Reading DDR physical address 0x80000000 (32 bytes)..."
    if {[catch {memory -read -fpga 0.A -channel 0 -file $readback_0x80000000 -start 0x80000000 -end 0x8000001F} err]} {
        puts "ERROR: Failed to read from 0x80000000: $err"
    } else {
        puts "Successfully read from 0x80000000"
        puts "First 32 bytes at DDR physical address 0x80000000:"
        set fp [open $readback_0x80000000 r]
        set line_count 0
        while {[gets $fp line] >= 0 && $line_count < 33} {
            puts "  $line"
            incr line_count
        }
        close $fp
    }

    # Also read from physical address 0 for comparison
    puts "\n========== Reading DDR at 0x00000000 (for comparison) =========="
    set readback_0x0 "/tmp/ddr_readback_0x0.hex"
    puts "Reading DDR physical address 0x00000000 (32 bytes)..."
    if {[catch {memory -read -fpga 0.A -channel 0 -file $readback_0x0 -start 0 -end 31} err]} {
        puts "ERROR: Failed to read from 0x0: $err"
    } else {
        puts "Successfully read from 0x0"
        puts "First 32 bytes at DDR physical address 0x00000000:"
        set fp [open $readback_0x0 r]
        set line_count 0
        while {[gets $fp line] >= 0 && $line_count < 33} {
            puts "  $line"
            incr line_count
        }
        close $fp
    }

    # Read DDR at 0x2e40-0x2e80 (data segment containing pointer at 0x2e58)
    puts "\n========== Reading DDR at 0x2e40 (where data pointer 0x2e58 should be) =========="
    set readback_0x2e40 "/tmp/ddr_readback_0x2e40.hex"
    puts "Reading DDR 0x2e40 - 0x2e80 (64 bytes)..."
    if {[catch {memory -read -fpga 0.A -channel 0 -file $readback_0x2e40 -start 0x2e40 -end 0x2e7F} err]} {
        puts "ERROR: Failed to read from 0x2e40: $err"
    } else {
        puts "Successfully read from 0x2e40"
        puts "Bytes at DDR 0x2e40 - 0x2e80:"
        set fp [open $readback_0x2e40 r]
        set line_count 0
        while {[gets $fp line] >= 0 && $line_count < 65} {
            puts "  $line"
            incr line_count
        }
        close $fp
    }

    puts "\n========== Running Normal Trace (without address fix) =========="
    set_trace_size 10000 rclk

    # Capture waveform using trace in VCD format
    # puts "Opening trace database (VCD format)..."
    # tracedb -open wave -vcd -overwrite

    # # Add signals to trace (add all traced signals)
    # puts "Adding signals to trace..."
    # trace_signals -add *

    # puts "Running simulation with trace..."


    # run $cycles rclk


    # puts "Uploading trace data..."
    # tracedb -upload

    # puts "Closing trace database..."
    # tracedb -close

    for {set i 0} {$i < 10} {incr i} {
        tracedb -open wave_mb$i -vcd -overwrite;
        trace_signals -add *;
        run 10000 rclk;
        tracedb -upload;
        tracedb -close;
        exec vcd2fst wave_mb$i.vcd wave_mb$i.fst
    }


    puts "Workload completed"
    puts "Waveform saved to wave.vcd"

    # Convert VCD to FST for smaller file size and faster loading
    # puts "Converting VCD to FST format..."
    # if {[catch {exec vcd2fst wave.vcd wave.fst} result]} {
    #     puts "Warning: vcd2fst conversion failed: $result"
    # } else {
    #     puts "FST waveform saved to wave.fst"
    #     puts "You can view it with: gtkwave wave.fst"
    # }
}
