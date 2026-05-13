# Load design from current directory
design .
# Connect to hardware server
hw_server . -location 0.A
# Configure DDR voltage (required for DDR4 to work)
# Bank 3,4,5 need 1.2V for DDR4
puts "Configuring DDR voltage..."
set_phc_vol -id 0.0 -bank 3,4,5 -voltage 1.2
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
puts "========== Initializing FPGA =========="
puts "Initializing reset sequence..."
puts "Opening reset-phase waveform capture..."
force io_sys_rstn 0
run 100rclk
force io_sys_rstn 1
puts "Polling DDR calibration status..."
set calib_done "'b0"
for {set i 0} {$i < 100000} {incr i} {
    run 100 rclk
    set calib_done [get_value io_init_calib_complete]
    if {$calib_done != "'b0"} {
        break
    }
}
if {$calib_done == "'b0"} {
    puts "ERROR: DDR calibration failed after 100 iterations"
    exit 1
}
puts "DDR calibration complete after $i iterations!"
puts "========== Loading Image to DDR =========="
# Back-door write (no -start, defaults to channel offset 0 which is CPU's 0x80000000)
memory -write -fpga 0.A -channel 0 -file /tmp/image.hex
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
after 10000
puts "========== Running Workload =========="
set_trace_size 1000 rclk
run 100 rclk
run 100 rclk
run 1000 rclk
get_time rclk
run 1000 rclk
puts "Save wave_ultra_04.vcd"   
run 200 rclk



