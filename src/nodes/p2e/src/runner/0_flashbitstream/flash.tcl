# Flash bitstream to FPGA
# This script is called by run.tcl

proc voltage_fpga_id {fpga_location} {
    if {![regexp {^([0-3])\.([A-E])$} $fpga_location -> board slot]} {
        error "invalid FPGA location: $fpga_location"
    }
    return "$board.[expr {[scan $slot %c] - [scan A %c]}]"
}

proc flash_bitstream {fpga_location {multi_fpga 0}} {
    puts "========== Flashing Bitstream =========="

    # Load design from current directory
    design .

    # Connect to hardware server
    if {$multi_fpga} {
        hw_server .
    } else {
        hw_server . -location $fpga_location
    }

    # Configure DDR voltage (required for DDR4 to work)
    # Bank 3,4,5 need 1.2V for DDR4
    puts "Configuring DDR voltage..."
    set_phc_vol -id [voltage_fpga_id $fpga_location] -bank 3,4,5 -voltage 1.2

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
