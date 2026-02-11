{ pkgs, bebop-host ? null }:

let
  hostSrc = builtins.path {
    path = ../../host;
    name = "bebop-host-tree";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "bebop-spike";
  version = "0.1.0";
  src = hostSrc;

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

    export SPIKE_SOURCE_DIR="$PWD/spike"
    export SPIKE_SRC="$SPIKE_SOURCE_DIR/riscv-isa-sim"
    export INSTALL_ROOT="$PWD/spike-build/install"
    mkdir -p "$INSTALL_ROOT"
    mkdir -p "$SPIKE_SRC/build"

    pushd "$SPIKE_SRC/build"
    ../configure \
      --prefix="$INSTALL_ROOT" \
      --with-boost=no \
      --with-boost-asio=no \
      --with-boost-regex=no
    popd
  '';

  buildPhase = ''
    export SPIKE_SOURCE_DIR="$PWD/spike"
    export SPIKE_SRC="$SPIKE_SOURCE_DIR/riscv-isa-sim"
    export INSTALL_ROOT="$PWD/spike-build/install"
    export RISCV="$INSTALL_ROOT"

    pushd "$SPIKE_SRC/build"
    make
    make install
    popd

    ln -sfn "$INSTALL_ROOT" "$SPIKE_SRC/install"

    mkdir -p "$PWD/spike-build/custom"
    pushd "$PWD/spike-build/custom"
    cmake "$SPIKE_SOURCE_DIR" -G Ninja \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX="$INSTALL_ROOT"
    cmake --build .
    cmake --install .
    popd
  '';

  installPhase = ''
    mkdir -p $out
    cp -r "$INSTALL_ROOT"/. $out/
  '';

  meta = with pkgs.lib; {
    description = "Spike RISC-V ISA simulator with Bebop extensions";
    homepage = "https://github.com/betrusted-io/buckyball";
    license = licenses.bsd3;
    platforms = platforms.linux;
  };
}