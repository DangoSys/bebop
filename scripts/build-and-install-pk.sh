#!/usr/bin/env bash
# Build riscv-pk and install pk so "make run" finds it. Run from repo root.
# If system toolchain lacks newlib (e.g. Ubuntu gcc-riscv64-unknown-elf), automatically
# downloads a prebuilt toolchain (xpack) and retries.
set -e
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PK_DIR="${RISCV_PK_DIR:-$HOME/riscv-pk}"
SPIKE_INSTALL="$ROOT/thirdparty/spike/build/install"
INSTALL_BIN="$SPIKE_INSTALL/riscv64-unknown-elf/bin"
TOOLCHAIN_CACHE="${TOOLCHAIN_CACHE:-$ROOT/.cache/riscv-toolchain}"
# Prebuilt toolchain (xpack, with newlib). Override with RISCV_TOOLCHAIN_URL.
XPACK_VERSION="13.3.0-2"
BASE_URL="https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases/download/v${XPACK_VERSION}"

if [ -x "$INSTALL_BIN/pk" ]; then
  echo "pk already installed at $INSTALL_BIN/pk"
  exit 0
fi

# Detect platform for prebuilt tarball
detect_arch() {
  case "$(uname -s)" in
    Darwin) OS=darwin ;;
    Linux)  OS=linux ;;
    *) echo "ERROR: unsupported OS" >&2; exit 1 ;;
  esac
  case "$(uname -m)" in
    x86_64|amd64) ARCH=x64 ;;
    aarch64|arm64) ARCH=arm64 ;;
    *) echo "ERROR: unsupported arch $(uname -m)" >&2; exit 1 ;;
  esac
  echo "${OS}-${ARCH}"
}

# Clear build dir; exit with hint if permission denied (e.g. was created by root).
clear_build_dir() {
  if [ -d "$PK_DIR/build" ]; then
    if ! rm -rf "$PK_DIR/build" 2>/dev/null; then
      echo "ERROR: cannot remove $PK_DIR/build (likely created by root)."
      echo "Run: sudo rm -rf $PK_DIR/build"
      echo "Then run this script again (no sudo needed)."
      exit 1
    fi
  fi
}

# Build pk: CC and HOST must be set by caller. Optional 3rd arg: extra CFLAGS to append in Makefile after configure (not passed to configure, so compiler check still passes).
do_configure_make() {
  local cc="$1" host="$2" cflags_extra="${3:-}" r f
  clear_build_dir
  mkdir -p "$PK_DIR/build"
  cd "$PK_DIR/build"
  export CC="$cc"
  export cross_compiling=yes
  set +e
  ../configure --prefix="${RISCV:-/usr}" --host="$host" ac_cv_prog_cc_cross=yes
  r=$?
  if [ $r -ne 0 ]; then
    cd - >/dev/null
    set -e
    return 1
  fi
  # riscv-pk uses empty march/mabi by default; xpack GCC 13+ needs -march=rv64gc_zicsr_zifencei
  if [ -n "$cflags_extra" ] && [ -f "Makefile" ]; then
    sed -i 's/^\([[:space:]]*march := -march=\).*/\1rv64gc_zicsr_zifencei/' Makefile
    sed -i 's/^\([[:space:]]*mabi := -mabi=\).*/\1lp64/' Makefile
  fi
  # Pass march/mabi on command line so they apply even if Makefile format differs
  local make_extra=""
  [ -n "$cflags_extra" ] && make_extra='march=-march=rv64gc_zicsr_zifencei mabi=-mabi=lp64'
  make -j"${NPROC:-$(nproc)}" $make_extra
  r=$?
  cd - >/dev/null
  set -e
  [ $r -eq 0 ]
}

# Returns 0 if config.log shows newlib/linker missing
is_newlib_failure() {
  [ -f "$PK_DIR/build/config.log" ] && \
    grep -q "cannot find crt0.o\|cannot find -lc\|cannot find -lgloss" "$PK_DIR/build/config.log" 2>/dev/null
}

if [ ! -d "$PK_DIR" ]; then
  echo "Cloning riscv-pk to $PK_DIR ..."
  git clone https://github.com/riscv-software-src/riscv-pk "$PK_DIR"
fi

# Prefer system riscv64-unknown-elf-gcc if available and working
USE_PREBUILT=""
if command -v riscv64-unknown-elf-gcc >/dev/null 2>&1; then
  echo "Building riscv-pk at $PK_DIR (using system riscv64-unknown-elf-gcc) ..."
  if do_configure_make "riscv64-unknown-elf-gcc" "riscv64-unknown-elf"; then
    : # success
  else
    if is_newlib_failure; then
      echo "System toolchain is missing newlib (crt0.o, libc, libgloss). Will use prebuilt toolchain."
      USE_PREBUILT=1
    else
      echo "ERROR: riscv-pk configure/make failed. See $PK_DIR/build/config.log"
      exit 1
    fi
  fi
else
  USE_PREBUILT=1
  echo "riscv64-unknown-elf-gcc not found. Using prebuilt toolchain."
fi

# Use prebuilt xpack toolchain (includes newlib)
if [ -n "$USE_PREBUILT" ] && [ ! -x "$PK_DIR/build/pk" ]; then
  ARCH=$(detect_arch)
  TARBALL="xpack-riscv-none-elf-gcc-${XPACK_VERSION}-${ARCH}.tar.gz"
  URL="${RISCV_TOOLCHAIN_URL:-$BASE_URL/$TARBALL}"
  EXTRACT_TO="$TOOLCHAIN_CACHE/xpack-${XPACK_VERSION}-${ARCH}"

  if [ ! -x "$EXTRACT_TO/bin/riscv-none-elf-gcc" ] && [ ! -x "$EXTRACT_TO/bin/riscv64-unknown-elf-gcc" ]; then
    mkdir -p "$TOOLCHAIN_CACHE"
    if [ -n "$RISCV_TOOLCHAIN_URL" ]; then
      DL_FILE="$TOOLCHAIN_CACHE/custom-toolchain.tar.gz"
      echo "Downloading toolchain from RISCV_TOOLCHAIN_URL ..."
      curl -fSL "$RISCV_TOOLCHAIN_URL" -o "$DL_FILE"
    else
      DL_FILE="$TOOLCHAIN_CACHE/$TARBALL"
      echo "Downloading prebuilt RISC-V toolchain (xpack ${XPACK_VERSION}) ..."
      curl -fSL "$URL" -o "$DL_FILE"
    fi
    echo "Extracting to $EXTRACT_TO ..."
    rm -rf "$EXTRACT_TO"
    tar -xzf "$DL_FILE" -C "$TOOLCHAIN_CACHE"
    # Find top-level dir that has bin/riscv*-elf-gcc (xpack or other prebuilt)
    TOP=$(cd "$TOOLCHAIN_CACHE" && for d in xpack-riscv-none-elf-gcc-*/; do
      [ -d "$d" ] && [ -x "${d}bin/riscv-none-elf-gcc" ] && echo "${d%/}" && break
    done)
    if [ -z "$TOP" ]; then
      TOP=$(cd "$TOOLCHAIN_CACHE" && for d in */; do
        [ -d "$d" ] && { [ -x "${d}bin/riscv-none-elf-gcc" ] || [ -x "${d}bin/riscv64-unknown-elf-gcc" ]; } && echo "${d%/}" && break
      done)
    fi
    if [ -z "$TOP" ]; then
      echo "ERROR: could not find riscv-none-elf-gcc or riscv64-unknown-elf-gcc in extracted tarball"
      exit 1
    fi
    mv "$TOOLCHAIN_CACHE/$TOP" "$EXTRACT_TO"
    rm -f "$DL_FILE"
  fi

  if [ -x "$EXTRACT_TO/bin/riscv-none-elf-gcc" ]; then
    PREBUILT_GCC="$EXTRACT_TO/bin/riscv-none-elf-gcc"
  else
    PREBUILT_GCC="$EXTRACT_TO/bin/riscv64-unknown-elf-gcc"
  fi
  HOST=$("$PREBUILT_GCC" -dumpmachine)
  # So configure/make find riscv-none-elf-objcopy, readelf, etc. (host objcopy cannot handle RISC-V ELF)
  export PATH="$EXTRACT_TO/bin:$PATH"
  # xpack GCC 13+ uses modular RISC-V spec: CSR insns need zicsr (and zifencei for fence.i)
  echo "Building riscv-pk at $PK_DIR (using prebuilt $PREBUILT_GCC, host=$HOST) ..."
  do_configure_make "$PREBUILT_GCC" "$HOST" "-march=rv64gc_zicsr_zifencei"
fi

if [ ! -x "$PK_DIR/build/pk" ]; then
  echo "ERROR: $PK_DIR/build/pk not found or not executable"
  if is_newlib_failure; then
    echo "Toolchain still missing newlib. Try setting RISCV_TOOLCHAIN_URL to a full toolchain tarball."
  fi
  exit 1
fi

mkdir -p "$INSTALL_BIN"
cp "$PK_DIR/build/pk" "$INSTALL_BIN/pk"
echo "Installed pk to $INSTALL_BIN/pk — you can now run: cd examples && make run"
