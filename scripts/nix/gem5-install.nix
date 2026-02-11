{ pkgs, bebopHost, gem5Src, spike ? null }:

let
  extrasSrc = builtins.path {
    path = ../../host/gem5;
    name = "bebop-gem5-extras";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "bebop-gem5";
  version = "0.1.0";
  src = gem5Src;

  nativeBuildInputs = with pkgs; [
    python3
    scons
    pkg-config
    m4
    git
  ];

  buildInputs = with pkgs; [
    boost
    protobuf
    gperftools
    zlib
    abseil-cpp
    dtc
    bebopHost
  ];

  # gem5 scons writes build artifacts into the source tree,
  # so we must copy src to a writable location first.
  # Also patch shebangs: gem5 scripts use #!/usr/bin/env python3
  # which doesn't exist in the nix sandbox.
  unpackPhase = ''
    cp -r $src gem5-src
    chmod -R u+w gem5-src
    cp -r ${extrasSrc}/BebopInOCPU ./BebopInOCPU
    cp -r ${extrasSrc}/simpoint ./simpoint
    chmod -R u+w simpoint
    patchShebangs gem5-src
    patchShebangs BebopInOCPU
  '';

  buildPhase = ''
    runHook preBuild

    export BEBOP_IPC_LIB=${bebopHost}/lib/libbebop_ipc.a
    export BEBOP_IPC_INCLUDE=${bebopHost}/include

    export PKG_CONFIG_PATH=${pkgs.lib.makeSearchPathOutput "lib" "lib/pkgconfig" [
      pkgs.protobuf
      pkgs.boost
      pkgs.gperftools
      pkgs.zlib
    ]}:$PKG_CONFIG_PATH
    export LIBRARY_PATH=${pkgs.lib.makeLibraryPath [ pkgs.abseil-cpp pkgs.gperftools pkgs.boost ]}:$LIBRARY_PATH

    # Build gem5
    cd gem5-src
    scons build/RISCV/gem5.opt -j$NIX_BUILD_CORES \
      EXTRAS=$(pwd)/../BebopInOCPU \
      LIBS="absl_log_internal_check_op absl_log_internal_conditions absl_log_internal_message absl_base absl_raw_logging_internal absl_strings absl_throw_delegate absl_string_view absl_spinlock_wait absl_int128 absl_log_severity"
    cd ..

    # Build SimPoint
    cd simpoint
    make clean || true
    make -j$NIX_BUILD_CORES
    cd ..

    runHook postBuild
  '';

  installPhase = ''
    mkdir -p $out/bin
    cp gem5-src/build/RISCV/gem5.opt $out/bin/
    mkdir -p $out/share/simpoint
    cp -r simpoint/* $out/share/simpoint/
  '';

  meta = with pkgs.lib; {
    description = "Bebop-integrated gem5 build";
    homepage = "https://github.com/betrusted-io/buckyball";
    license = licenses.bsd3;
    platforms = platforms.linux;
  };
}
