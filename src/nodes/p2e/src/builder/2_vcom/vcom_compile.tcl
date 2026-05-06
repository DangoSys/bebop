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

# Add CLINT registers for runtime debugging and control
# These allow us to read/write CLINT's mtime, mtimecmp, and ipi registers at runtime
write_net -add {P2ETop.top.soc.clint_domain.clint.time_0}
write_net -add {P2ETop.top.soc.clint_domain.clint.timecmp_0}
write_net -add {P2ETop.top.soc.clint_domain.clint.ipi_0}

# Add CSR registers for runtime debugging and control
# These allow us to enable/disable interrupts at runtime
write_net -add {P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mie}
write_net -add {P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mstatus_mie}

# Define readable nets for runtime monitoring
read_net -add {io_init_calib_complete}

# Add trace for waveform capture
# IMPORTANT: Keep depth low to avoid suspending clocks (max 5000 probes per FPGA)
# Only trace the core (CPU) to reduce probe count and avoid clock suspension
trace_net -add P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0 -depth 3

# Create clock constraints
# Reference config (VCU118) uses 5MHz for the main SoC clock
#
# IMPORTANT: vcom uses -sig_name and -frequency syntax (not Vivado XDC syntax)
# The signal name should be the internal clock signal after the differential buffer
# From P2ETopBlackBox, the input is: user_clk (single-ended, after IBUFDS)
# The full hierarchical path is: xepic_vvac_top.P2ETop.top.user_clk

# Main SoC clock (internal signal after IBUFDS) - 5MHz
# The clock signal in P2ETopBlackBox is 'user_clk' (not 'io_user_clk')
create_clock -sig_name ${top_module}.P2ETop.top.user_clk -frequency 5Mhz

# Enable design rule mode
set_dr_mode -add enable

# Generate FPGA design
design_edit
design_generation
