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

    run $cycles rclk
}
