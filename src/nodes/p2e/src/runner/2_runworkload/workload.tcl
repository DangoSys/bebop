# Run workload on FPGA
# This script is called by run.tcl

proc run_workload {fpga_location ddr_channel image_path} {
    puts "========== Running Workload =========="

    # Load image to DDR memory via backdoor
    puts "Loading image to DDR memory..."
    puts "  FPGA: $fpga_location"
    puts "  Channel: $ddr_channel"
    puts "  Image: $image_path"

    memory -write -fpga $fpga_location -channel $ddr_channel -file $image_path

    # Run simulation
    puts "Running workload..."
    run 55000000 rclk

    # Optional: Capture waveform
    # set_trace_size 30000 rclk
    # tracedb -open wave -xedb -overwrite
    # trace_signals -add *
    # run 30000 rclk
    # tracedb -upload
    # tracedb -close

    puts "Workload completed"
}
