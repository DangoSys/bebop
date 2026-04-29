#!/bin/bash
set -euo pipefail

# P2E VVAC Build Script
# This script builds the P2E VVAC library (libvCtb.so) for bebop-p2e
# Based on the successful p2e_control_path example

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BEBOP_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OUT_DIR="$BEBOP_ROOT/out"

# Check required environment variable
if [ -z "${ARCH_CONFIG:-}" ]; then
    echo "Error: ARCH_CONFIG environment variable is required"
    echo "Example: ARCH_CONFIG=sims.p2e.P2EToyConfig"
    exit 1
fi

# Check if Verilog files exist
BUILD_DIR="$BEBOP_ROOT/../arch/build/$ARCH_CONFIG"
if [ ! -d "$BUILD_DIR" ]; then
    echo "Error: Verilog build directory not found: $BUILD_DIR"
    echo "Please generate Verilog first with: ARCH_CONFIG=$ARCH_CONFIG"
    exit 1
fi

if [ ! -f "$BUILD_DIR/P2EHarness.sv" ]; then
    echo "Error: P2EHarness.sv not found in $BUILD_DIR"
    echo "This doesn't look like a P2E build"
    exit 1
fi

# Source environment variables
echo "Sourcing environment from $SCRIPT_DIR/sourceme.sh"
source "$SCRIPT_DIR/sourceme.sh"

# CRITICAL: Unset LD_LIBRARY_PATH after sourcing to prevent ALL tools (vvac, clang-format, cmake)
# from linking to HPE's old libstdc++. This allows them to use Nix's modern libstdc++.
unset LD_LIBRARY_PATH

# Create a dummy clang-format that does nothing to avoid libstdc++ conflicts
# vvac's Makefile calls clang-format which would link to HPE's old libstdc++
mkdir -p "$OUT_DIR/.dummy_bin"
cat > "$OUT_DIR/.dummy_bin/clang-format" << 'DUMMY_EOF'
#!/bin/bash
# Dummy clang-format that does nothing to avoid libstdc++ conflicts
exit 0
DUMMY_EOF
chmod +x "$OUT_DIR/.dummy_bin/clang-format"
export PATH="$OUT_DIR/.dummy_bin:$PATH"

# Verify required tools
command -v vvac >/dev/null || { echo "Error: vvac not found"; exit 1; }
command -v cmake >/dev/null || { echo "Error: cmake not found"; exit 1; }

# Create output directory
mkdir -p "$OUT_DIR"
cd "$OUT_DIR"

# Generate file list
echo "Generating file list..."
FLIST="$OUT_DIR/p2e_vvac_filelist.f"
find "$BUILD_DIR" -name "*.v" -o -name "*.sv" | sort > "$FLIST"
echo "Found $(wc -l < "$FLIST") Verilog files"

# Run VVAC (will fail at cmake step due to LD_LIBRARY_PATH, but that's OK)
# We'll manually compile the DPI-C library afterwards
echo "Running VVAC..."
vvac -bc -f "$FLIST" -top P2EHarness 2>&1 | tee vvac_build.log || true

# Check if VVAC generated the code (even if cmake failed)
if [ ! -d "$OUT_DIR/vvac.tmp/dpic" ]; then
    echo "Error: VVAC failed to generate dpic directory"
    exit 1
fi

echo "VVAC code generation completed (cmake errors are expected and will be fixed)"

# Patch CMakeLists.txt for cmake compatibility
CMAKE_LISTS="$OUT_DIR/vvac.tmp/dpic/CMakeLists.txt"
if [ -f "$CMAKE_LISTS" ]; then
    echo "Patching CMakeLists.txt for cmake compatibility..."
    # Fix cmake_minimum_required (cmake 4.x requires >= 3.5)
    sed -i 's|cmake_minimum_required(VERSION 3.4.3)|cmake_minimum_required(VERSION 3.5)|' "$CMAKE_LISTS"
    # Fix cmake_path (requires cmake 3.20+, use simple assignment instead)
    sed -i 's|cmake_path(NORMAL_PATH LD OUTPUT_VARIABLE LD_OUT)|set(LD_OUT "${LD}")  # cmake_path requires cmake 3.20+, use simple assignment instead|' "$CMAKE_LISTS"
fi

# Patch stub.h to fix vvac code generation bug (missing type for parameter i1)
STUB_H="$OUT_DIR/vvac.tmp/dpic/ctb_gen/stub.h"
if [ -f "$STUB_H" ]; then
    echo "Patching stub.h to fix vvac code generation bug..."
    # Fix p2e_uart_write: add missing uint32_t type for parameter i1
    sed -i 's|p2e_uart_write(uint32_t i0,  i1);|p2e_uart_write(uint32_t i0, uint32_t i1);|' "$STUB_H"
fi

# Build DPI-C library with cmake (manually, not through vvac's Makefile)
# This avoids LD_LIBRARY_PATH issues that occur when vvac's Makefile calls cmake
echo "Building DPI-C library..."
cd "$OUT_DIR/vvac.tmp/dpic"
mkdir -p build
cd build

# LD_LIBRARY_PATH already unset earlier, cmake will use Nix's libstdc++
cmake .. -D_ARM_=ON -DCMAKE_BUILD_TYPE=Release
make -j8
make install

# Check if library was generated
LIBVCTB_SRC="$OUT_DIR/vvac.tmp/dpic/extern/lib/libvCtb.so"
if [ ! -f "$LIBVCTB_SRC" ]; then
    echo "Error: libvCtb.so not found at $LIBVCTB_SRC"
    exit 1
fi

# Copy to expected location for cargo
cp "$LIBVCTB_SRC" "$OUT_DIR/libvCtb.so"

echo "✓ Build complete!"
echo "  libvCtb.so: $OUT_DIR/libvCtb.so"
echo ""
echo "You can now run: cargo build -p bebop-p2e --config=\"env.ARCH_CONFIG='$ARCH_CONFIG'\""
