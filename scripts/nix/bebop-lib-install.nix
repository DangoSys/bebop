{ pkgs }:

let
  ipcSrc = builtins.path {
    path = ../../host/ipc;
    name = "bebop-ipc-src";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "bebop-ipc";
  version = "0.1.0";
  src = ipcSrc;

  nativeBuildInputs = with pkgs; [ cmake ];

  # ipc source only uses standard libraries, no external deps needed

  configurePhase = ''
    runHook preConfigure
    cmake -S . -B build \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX=$out \
      -DCMAKE_INSTALL_RPATH=$out/lib
    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild
    cmake --build build
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    cmake --install build
    mkdir -p $out/include
    cp -r include/ipc $out/include/
    mkdir -p $out/lib
    cp build/libbebop_ipc.a $out/lib/
    runHook postInstall
  '';

  meta = with pkgs.lib; {
    description = "Bebop IPC static library";
    license = licenses.asl20;
    platforms = platforms.linux;
  };
}
