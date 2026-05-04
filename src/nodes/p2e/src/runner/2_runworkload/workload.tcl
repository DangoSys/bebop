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
    puts "Running for $cycles cycles and capturing waveform..."

    # Set trace size (capture last N cycles)
    set trace_cycles 100000
    puts "Setting trace size to $trace_cycles cycles"
    set_trace_size $trace_cycles rclk

    # Open trace database
    puts "Opening trace database..."
    tracedb -open wave -xedb -overwrite

    # Add signals to trace
    puts "Adding signals to trace..."
    trace_signals -add *

    # Run simulation
    puts "Running simulation for $cycles cycles..."
    run $cycles rclk

    # Upload and close trace
    puts "Uploading trace data..."
    tracedb -upload

    puts "Closing trace database..."
    tracedb -close

    puts "Workload completed"
    puts "Waveform saved to wave.xedb"
    puts "Use: fusiondebug -wdb wave.xedb to view waveform"
}
