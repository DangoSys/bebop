# Flash bitstream to FPGA
# This script is called by run.tcl

proc flash_bitstream {fpga_location {multi_fpga 0}} {
    puts "========== Flashing Bitstream =========="

    # Load design from current directory
    design .

    # Connect to hardware server
    if {$multi_fpga} {
        hw_server .
        puts "Configuring DDR voltage..."
        set_phc_vol -id 0.0 -bank 3,4,5 -voltage 1.2
    } else {
        # Prefer the requested location, then fall back to another board when
        # it is already reserved by another user.  The design-side FPGA remains
        # 0.A; hw_server establishes the logical-to-physical mapping.
        set fpga_candidates [list $fpga_location]
        foreach candidate {0.A 1.A 2.A 3.A 4.A} {
            if {[lsearch -exact $fpga_candidates $candidate] < 0} {
                lappend fpga_candidates $candidate
            }
        }

        set fpga_selected 0
        set last_error ""
        foreach candidate $fpga_candidates {
            puts "Trying FPGA location $candidate..."
            if {[catch {hw_server . -location $candidate} last_error]} {
                puts "FPGA $candidate connection failed: $last_error"
                catch {hw_server -release}
                continue
            }

            puts "Configuring DDR voltage on $candidate..."
            if {[catch {set_phc_vol -id 0.0 -bank 3,4,5 -voltage 1.2} last_error]} {
                puts "FPGA $candidate is unavailable: $last_error"
                catch {hw_server -release}
                continue
            }

            set fpga_selected 1
            puts "Selected FPGA location $candidate"
            break
        }

        if {!$fpga_selected} {
            error "No available FPGA found in $fpga_candidates; last error: $last_error"
        }
    }

    # Download bitstream to FPGA
    download
    after 1000

    puts "Bitstream flashed successfully"

    # Signal host that flash is done and wait for host init
    set flag_file "flash_done.flag"
    set host_init_file "host_init_done.flag"

    set fd [open $flag_file w]
    close $fd
    puts "Waiting for host to initialize CTB..."

    while {![file exists $host_init_file]} {
        after 100
    }
    puts "Host CTB initialization complete"
}
