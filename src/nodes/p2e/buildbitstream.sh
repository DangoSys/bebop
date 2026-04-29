#!/bin/bash
set -euo pipefail

# P2E Bitstream Build Script
# This script generates FPGA bitstream for P2E after libvCtb.so is built
# Build flow: vsyn (synthesis) -> vcom (system compile) -> make (PNR)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BEBOP_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OUT_DIR="$BEBOP_ROOT/out"

# Check required environment variable
if [ -z "${ARCH_CONFIG:-}" ]; then
    echo "Error: ARCH_CONFIG environment variable is required"
    echo "Example: ARCH_CONFIG=sims.p2e.P2EToyConfig"
    exit 1
fi

# Check if VVAC build completed
if [ ! -d "$OUT_DIR/vvacDir" ]; then
    echo "Error: VVAC build not found at $OUT_DIR/vvacDir"
    echo "Please run build.sh first to generate libvCtb.so"
    exit 1
fi

VVAC_DIR="$OUT_DIR/vvacDir"
VVAC_FILELIST="$VVAC_DIR/vvac_by_mod/filelist"

if [ ! -f "$VVAC_FILELIST" ]; then
    echo "Error: VVAC filelist not found at $VVAC_FILELIST"
    echo "VVAC build may be incomplete"
    exit 1
fi

# Source environment variables
echo "Sourcing environment from $SCRIPT_DIR/sourceme.sh"
source "$SCRIPT_DIR/sourceme.sh"

# Verify required tools
command -v vsyn >/dev/null || { echo "Error: vsyn not found"; exit 1; }
command -v vcom >/dev/null || { echo "Error: vcom not found"; exit 1; }

# Step 1: Synthesis with vsyn
echo "========================================="
echo "Step 1: Running vsyn (synthesis)..."
echo "========================================="
cd "$OUT_DIR"

NETLIST_FILE="$OUT_DIR/p2e_top.vm"
TOP_MODULE="xepic_vvac_top"

vsyn -F "$VVAC_FILELIST" -top "$TOP_MODULE" -o "$NETLIST_FILE" 2>&1 | tee vsyn_build.log

if [ ! -f "$NETLIST_FILE" ]; then
    echo "Error: vsyn failed to generate netlist at $NETLIST_FILE"
    exit 1
fi

echo "✓ Synthesis complete: $NETLIST_FILE"

# Step 2: System compile with vcom
echo ""
echo "========================================="
echo "Step 2: Running vcom (system compile)..."
echo "========================================="

# Create vcom TCL script
VCOM_TCL="$OUT_DIR/vcom_compile.tcl"
cat > "$VCOM_TCL" << 'EOF'
# P2E System Compile TCL Script
# This script configures FPGA compilation settings for vcom

# Set target FPGA device (adjust based on your hardware)
set_option -part "xcu280-fsvh2892-2L-e"

# Set top module
set_option -top_module xepic_vvac_top

# Set output directory
set_option -output_dir fpgaCompDir

# Enable timing-driven compilation
set_option -timing_driven 1

# Set frequency constraint (adjust based on your design)
create_clock -period 10.0 [get_ports clk]

# Run compilation
compile
EOF

vcom "$VCOM_TCL" 2>&1 | tee vcom_build.log

FPGA_DIR="$OUT_DIR/fpgaCompDir"
if [ ! -d "$FPGA_DIR" ]; then
    echo "Error: vcom failed to generate FPGA directory at $FPGA_DIR"
    exit 1
fi

echo "✓ System compile complete: $FPGA_DIR"

# Step 3: Place and Route with make
echo ""
echo "========================================="
echo "Step 3: Running PNR (place and route)..."
echo "========================================="

cd "$FPGA_DIR"

# Check if Makefile exists
if [ ! -f "Makefile" ]; then
    echo "Error: Makefile not found in $FPGA_DIR"
    echo "vcom may have failed to generate proper build files"
    exit 1
fi

# Run make to generate bitstream
make all 2>&1 | tee "$OUT_DIR/pnr_build.log"

# Find generated bitstream
BITSTREAM=$(find "$FPGA_DIR" -name "*.bit" -type f | head -1)

if [ -z "$BITSTREAM" ]; then
    echo "Error: No bitstream (.bit) file found in $FPGA_DIR"
    echo "PNR may have failed"
    exit 1
fi

# Copy bitstream to output directory with standard name
FINAL_BITSTREAM="$OUT_DIR/p2e_${ARCH_CONFIG##*.}.bit"
cp "$BITSTREAM" "$FINAL_BITSTREAM"

echo ""
echo "========================================="
echo "✓ Bitstream build complete!"
echo "========================================="
echo "  Bitstream: $FINAL_BITSTREAM"
echo "  Size: $(du -h "$FINAL_BITSTREAM" | cut -f1)"
echo ""
echo "Next steps:"
echo "  1. Flash to FPGA: Use vdbg or flashbitstream tool"
echo "  2. Run simulation: cargo run -p bebop-p2e"
echo ""