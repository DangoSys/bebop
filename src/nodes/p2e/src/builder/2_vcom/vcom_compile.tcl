# P2E System Compile TCL Script
# Based on p2e_control_path example

set top_module "xepic_vvac_top"

# Read empty module definitions from original Verilog files (not VVAC processed)
# These modules are optimized away by vsyn but still referenced in the netlist
set arch_build_dir "/home/wanghui/Code/buckyball/arch/build/sims.p2e.P2EToyConfig"
if {[file exists "$arch_build_dir/IntSyncCrossingSource_n1x1_Registered.sv"]} {
    design_read -netlist $arch_build_dir/IntSyncCrossingSource_n1x1_Registered.sv
}
if {[file exists "$arch_build_dir/NullIntSource.sv"]} {
    design_read -netlist $arch_build_dir/NullIntSource.sv
}

# Read netlist and Vivado vcom library
design_read -netlist ./xepic_vvac_top.vm
design_read -netlist $env(VSYN_HOME)/share/verilog/vtech_vivado_vcom.v

# Load design with top module
design_load -top ${top_module}

# Set register visibility for debugging
registers_visibility -effort low

# Configure host channel
design_edit_option -add {host_channel hac}

# Map VVAC configuration directory
vvac_cfg_map -dir ./vvacDir

# Load hardware configuration
emulator_spec -add {file ./hw-config.hdf}

# Create clock constraint (100MHz default, adjust if needed)
create_clock -sigName ${top_module}.clock -frequency 100Mhz

# Set emulator utilization
emulator_util -add {default 0}

# Enable design rule mode
set_dr_mode -add enable

# Generate FPGA design
design_edit
design_generation
