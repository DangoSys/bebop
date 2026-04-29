#!/bin/bash
# ============================================================================
# P2E Environment Setup Script
# ============================================================================
# This script sets up all required environment variables for P2E simulation
# Usage: source ./sourceme.sh

# Set HPEC home directory
export HPEC_HOME="${HPEC_HOME:-/home/x-epic/hpe-24.12.01.s008}"

# Source HPE setup script (sets VCOM_HOME, VDBG_HOME, VSYN_HOME, etc.)
# source $HPEC_HOME/.setup.sh
export PATH="$HPEC_HOME"/bin:"$PATH"
export VCOM_HOME="$HPEC_HOME"
export VDBG_HOME="$HPEC_HOME"
export VSYN_HOME="$HPEC_HOME"
export VVAC_HOME="$HPEC_HOME"
export XRAM_HOME="$HPEC_HOME"/public/xram
export DBGIP_HOME="$HPEC_HOME"/share/pnr/dbg_ip
export HPE_HOME="$HPEC_HOME"
export XEPIC_IP_HOME="$HPE_HOME/netlist_macro_packages"
export XEPIC_VTECH_HOME="$HPE_HOME"/share/verilog

# HPEC tools
# export PATH="$HPEC_HOME/platform/linux64/bin:$PATH"
# export PATH="$HPEC_HOME/bin:$PATH"
export PATH="$HPE_HOME/tools/xwave/bin:$PATH"


# Vivado tool paths
export VIVADO_PATH="${VIVADO_PATH:-/home/tools/vivado/Vivado/2022.2}"
export PATH=$VIVADO_PATH/bin:$PATH
export PATH=$VIVADO_PATH/gnu/microblaze/lin/bin:$PATH

# License configuration
export RLM_LICENSE="${RLM_LICENSE:-5053@192.168.99.15}"
export LM_LICENSE_FILE="${LM_LICENSE_FILE:-/home/tools/vivado/license.lic}"

# Project-specific environment variables. build.rs runs tools from bebop/out,
# so the default CASE_HOME is the current working directory.
export CASE_HOME="${CASE_HOME:-$(pwd)}"
export TBSERVER_ETC="$CASE_HOME/vvacDir/runtimeDir/"
export VVAC_GEN="$CASE_HOME/vvacDir/vvac_by_mod/"
export top_module="xepic_vvac_top"
export VVAC_WORK_DIR="$CASE_HOME/vvacDir/"
export NEWBACKDOOR=1
