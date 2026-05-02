# Flash bitstream to FPGA
# This script is called by run.tcl

proc flash_bitstream {fpga_location} {
    puts "========== Flashing Bitstream =========="

    # Load design from current directory
    design .

    # Connect to hardware server
    hw_server . -location $fpga_location

    # Download bitstream to FPGA
    download

    puts "Bitstream flashed successfully"
}
