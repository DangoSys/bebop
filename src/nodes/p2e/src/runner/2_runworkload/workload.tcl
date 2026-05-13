# Run workload on FPGA

proc load_image {fpga_location ddr_channel image_path} {
    puts "========== Loading Image to DDR =========="
    puts "  FPGA: $fpga_location"
    puts "  Channel: $ddr_channel"
    puts "  Image: $image_path"

    # Back-door write (no -start, defaults to channel offset 0 which is CPU's 0x80000000)
    memory -write -fpga $fpga_location -channel $ddr_channel -file $image_path
    puts "Image write completed"

    # Back-door read to verify entire image
    puts "========== Verifying DDR Content =========="
    set readback_file "[file dirname $image_path]/ddr_readback_full.hex"

    # Calculate image size from hex file (line count - 1 for @0 header)
    set fp [open $image_path r]
    set line_count 0
    while {[gets $fp line] >= 0} {
        incr line_count
    }
    close $fp
    set image_bytes [expr $line_count - 1]
    set end_addr [expr $image_bytes - 1]

    puts "Image size: $image_bytes bytes (0x[format %x $image_bytes])"
    # puts "Reading back DDR from offset 0 to $end_addr..."

    # if {[catch {memory -read -fpga $fpga_location -channel $ddr_channel -file $readback_file -start 0 -end $end_addr} err]} {
    #     puts "ERROR: DDR readback failed: $err"
    # } else {
    #     puts "DDR readback completed: $readback_file"

        # Compare first 64 bytes
    #     puts "First 64 bytes comparison:"
    #     set fp_orig [open $image_path r]
    #     set fp_read [open $readback_file r]

    #     # Skip @0 header in both files
    #     gets $fp_orig
    #     gets $fp_read

    #     set mismatch 0
    #     for {set i 0} {$i < 64} {incr i} {
    #         if {[gets $fp_orig orig_line] < 0 || [gets $fp_read read_line] < 0} {
    #             break
    #         }
    #         set orig_byte [string toupper [string trim $orig_line]]
    #         set read_byte [string toupper [string trim $read_line]]
    #         if {$orig_byte ne $read_byte} {
    #             puts "  MISMATCH at offset $i: wrote $orig_byte, read $read_byte"
    #             incr mismatch
    #         }
    #     }
    #     close $fp_orig
    #     close $fp_read

    #     if {$mismatch == 0} {
    #         puts "  First 64 bytes match perfectly!"
    #     } else {
    #         puts "  WARNING: Found $mismatch mismatches in first 64 bytes"
    #     }

    #     # Also check critical addresses where execution hung (0x9da area)
    #     puts "\nChecking critical address 0x9da (where PC hung):"
    #     set fp_read [open $readback_file r]
    #     gets $fp_read  ;# skip @0
    #     for {set i 0} {$i < 0x9da} {incr i} {
    #         gets $fp_read
    #     }
    #     for {set i 0} {$i < 8} {incr i} {
    #         gets $fp_read byte
    #         puts "  0x[format %03x [expr 0x9da + $i]]: $byte"
    #     }
    #     close $fp_read
    # }

    # 10s
    after 10000
}

proc run_workload {cycles} {
    puts "========== Running Workload =========="

    run 20000rclk
}
