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

# Set FPGA resource utilization limit
emulator_util -add {default 70}

# Define writable nets for runtime control
write_net -add {io_sys_rstn}

# Add AXI/DDR interconnect signals for debugging
# These allow us to monitor and control the AXI bus between CPU and DDR
# mem_awaddr_soc/mem_araddr_soc: 32-bit address from DigitalTop (CPU view, 0x80000000-based)
# mem_awaddr/mem_araddr:         64-bit address to DDR (after p2e_mem_addr_translator, 0x0-based)
# write_net -add {P2ETop.top.mem_awaddr_soc}
# write_net -add {P2ETop.top.mem_awaddr}
# write_net -add {P2ETop.top.mem_araddr_soc}
# write_net -add {P2ETop.top.mem_araddr}
# write_net -add {P2ETop.top.mem_awvalid}
# write_net -add {P2ETop.top.mem_arvalid}
# write_net -add {P2ETop.top.mem_wdata}
# write_net -add {P2ETop.top.mem_rdata}

# Add CSR registers for runtime debugging and control
# These allow us to enable/disable interrupts at runtime
# write_net -add {P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mie}
# write_net -add {P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0.csr.reg_mstatus_mie}

# Add BootROM registers for runtime debugging
# These allow us to inspect and modify BootROM contents at runtime for debugging
# This is useful for verifying that the BootROM was correctly initialized
# write_net -add {P2ETop.top.soc.bootrom_domain.bootrom.rom_0}
# write_net -add {P2ETop.top.soc.bootrom_domain.bootrom.rom_1}
# write_net -add {P2ETop.top.soc.bootrom_domain.bootrom.rom_2}
# write_net -add {P2ETop.top.soc.bootrom_domain.bootrom.rom_3}

#===--------------------------------------------------------===#
# Define readable nets for runtime monitoring
# Dont Remove this net, it is used for simulation
read_net -add {io_init_calib_complete}
#===--------------------------------------------------------===#

# Add trace for waveform capture
# trace_net -add P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.cores_0 -depth 5
trace_net -add P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile.accelerators_0 -depth 5
# trace_net -add P2ETop.top.soc.tile_prci_domain.element_reset_domain_bbtile -depth 5

# Add BootROM trace for debugging ROM initialization issues
# Trace depth 2 to capture ROM registers and their connections
# trace_net -add P2ETop.top.soc.bootrom_domain -depth 4

# Add P2ETop.top level signals (depth 1) to capture AXI/DDR interconnect signals
# This includes mem_awaddr, mem_araddr, mem_wdata, mem_rdata etc.
# trace_net -add P2ETop.top -depth 3
# trace_net -add P2ETop.top -depth 5

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

# Enable this when this board is reousrces are limited for design
# memory_options -add{bram_balance SMART}

# Generate FPGA design
design_edit
design_generation
