{ pkgs }:

let
  hostSrc = builtins.path {
    path = ../../host;
    name = "bebop-host-src";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "bebop-host";
  version = "0.1.0";
  src = hostSrc;

  nativeBuildInputs = with pkgs; [
    cmake
    ninja
  ];

  buildInputs = with pkgs; [
    boost
    dtc
  ];

  configurePhase = ''
    runHook preConfigure

    export RISCV="$PWD/riscv-placeholder"
    mkdir -p "$RISCV/include" "$RISCV/lib"

    cmake -S . -B build -G Ninja \
      -DCMAKE_INSTALL_PREFIX=$out \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_RPATH=$out/lib
  '';

  buildPhase = ''
    cmake --build build
  '';

  installPhase = ''
    cmake --install build
    mkdir -p $out/include
    cp -r ipc/include $out/include/ipc
  '';

  meta = with pkgs.lib; {
    description = "Bebop host IPC libraries";
    homepage = "https://github.com/betrusted-io/buckyball";
    license = licenses.asl20;
    platforms = platforms.linux;
  };
}