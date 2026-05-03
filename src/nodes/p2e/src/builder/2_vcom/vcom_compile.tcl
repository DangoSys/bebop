# P2E System Compile TCL Script
# Based on p2e_ddr4_backdoor example

set top_module "xepic_vvac_top"

# Read netlist
design_read -netlist ./xepic_vvac_top.vm

# Configure DDR4 controller as netlist macro
# This tells vcom to treat xepic_ddr4_dc1 as a pre-compiled IP core
set nnmf_path "$env(HPE_HOME)/netlist_macro_packages"
netlistmacro -instance ${top_module}.P2ETop.top.ddr -package ${nnmf_path}/xepic_ddr4_dc1 -location 0.A -channel 0

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

# Set FPGA resource utilization limit (70% like p2e_ddr4_backdoor)
emulator_util -add {default 70}

# Define writable nets for runtime control
write_net -add {io_sys_rstn}

# Define readable nets for runtime monitoring
read_net -add {io_init_calib_complete}

# Create clock constraint (100MHz default)
# Note: The top-level clock signal is 'io_user_clk', not 'user_clk'
create_clock -sig_name ${top_module}.io_user_clk -frequency 100Mhz

# Enable design rule mode
set_dr_mode -add enable

# Generate FPGA design
design_edit
design_generation
