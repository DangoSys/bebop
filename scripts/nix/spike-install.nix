{ pkgs, bebopHost ? null, spikeSrc }:

let
  customExt = builtins.path {
    path = ../../host/spike/customext;
    name = "bebop-customext";
  };

  ipcSrc = builtins.path {
    path = ../../host/ipc;
    name = "bebop-ipc-src";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "bebop-spike";
  version = "0.1.0";
  src = spikeSrc;

  nativeBuildInputs = with pkgs; [
    autoconf
    automake
    libtool
    pkg-config
    cmake
    ninja
  ];

  buildInputs = with pkgs; [
    gmp
    mpfr
    libmpc
    zlib
    dtc
  ];

  configurePhase = ''
    runHook preConfigure

    export SPIKE_ROOT="$PWD"
    export INSTALL_ROOT="$SPIKE_ROOT/install"
    mkdir -p "$INSTALL_ROOT"

    mkdir -p "$SPIKE_ROOT/build-spike"
    cd "$SPIKE_ROOT/build-spike"
    "$SPIKE_ROOT/configure" \
      --prefix="$INSTALL_ROOT" \
      --with-boost=no \
      --with-boost-asio=no \
      --with-boost-regex=no
  '';

  buildPhase = ''
    export SPIKE_ROOT="$NIX_BUILD_TOP/$sourceRoot"
    export INSTALL_ROOT="$SPIKE_ROOT/install"
    export RISCV="$INSTALL_ROOT"

    # 1. Build and install spike itself
    cd "$SPIKE_ROOT/build-spike"
    make -j$NIX_BUILD_CORES
    make install

    # 2. Prepare customext source tree with ipc alongside it
    #    customext CMakeLists expects paths relative to its own dir:
    #      SPIKE_ROOT = ../riscv-isa-sim
    #      SPIKE_PREFIX = SPIKE_ROOT/install
    #    So we create: work/spike/riscv-isa-sim/install -> $INSTALL_ROOT
    cd "$SPIKE_ROOT"
    mkdir -p work/spike/riscv-isa-sim work/ipc
    ln -sfn "$INSTALL_ROOT" work/spike/riscv-isa-sim/install
    cp -r ${customExt} work/spike/customext
    chmod -R u+w work/spike/customext
    cp -r ${ipcSrc}/. work/ipc/
    chmod -R u+w work/ipc

    # 3. Build customext (produces libbebop.so)
    mkdir -p work/spike/customext/build
    cd work/spike/customext/build
    cmake .. \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX="$INSTALL_ROOT"
    make -j$NIX_BUILD_CORES
    make install
  '';

  installPhase = ''
    mkdir -p $out
    cp -r "$NIX_BUILD_TOP/$sourceRoot/install"/. $out/
  '';

  meta = with pkgs.lib; {
    description = "Spike RISC-V ISA simulator with Bebop extensions";
    homepage = "https://github.com/betrusted-io/buckyball";
    license = licenses.bsd3;
    platforms = platforms.linux;
  };
}
