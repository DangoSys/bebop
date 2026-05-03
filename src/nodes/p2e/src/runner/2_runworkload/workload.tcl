# Run workload on FPGA
# This script is called by run.tcl

proc load_image {fpga_location ddr_channel image_path} {
    puts "========== Loading Image to DDR =========="
    puts "  FPGA: $fpga_location"
    puts "  Channel: $ddr_channel"
    puts "  Image: $image_path"

    memory -write -fpga $fpga_location -channel $ddr_channel -file $image_path
    puts "Image loaded successfully"
}

proc run_workload {cycles} {
    puts "========== Running Workload =========="
    puts "Running for $cycles cycles..."

    run $cycles rclk

    puts "Workload completed"
}
