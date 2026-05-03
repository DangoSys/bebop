# Flash bitstream to FPGA
# This script is called by run.tcl

proc flash_bitstream {fpga_location} {
    puts "========== Flashing Bitstream =========="

    # Load design from current directory
    design .

    # Connect to hardware server
    hw_server . -location $fpga_location

    # Configure DDR voltage (required for DDR4 to work)
    # Bank 3,4,5 need 1.2V for DDR4
    puts "Configuring DDR voltage..."
    set_phc_vol -id 0.0 -bank 3,4,5 -voltage 1.2

    # Download bitstream to FPGA
    download

    puts "Bitstream flashed successfully"
}
