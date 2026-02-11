{ pkgs, bebop-host, spike ? null }:

let
  hostSrc = builtins.path {
    path = ../../host;
    name = "bebop-host-tree";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "bebop-gem5";
  version = "0.1.0";
  src = hostSrc;

  nativeBuildInputs = with pkgs; [
    python3
    scons
    pkg-config
  ];

  buildInputs = with pkgs; [
    boost
    protobuf
    gperftools
    zlib
    abseil-cpp
    dtc
    bebop-host
  ];

  buildPhase = ''
    runHook preBuild

    export HOST_ROOT=$PWD
    export BEBOP_HOST_LIB=$HOST_ROOT
    export BEBOP_IPC_LIB=${bebop-host}/lib/libbebop_ipc.a
    export BEBOP_IPC_INCLUDE=${bebop-host}/include

    export PKG_CONFIG_PATH=${pkgs.lib.makeSearchPathOutput "lib" "lib/pkgconfig" [
      pkgs.protobuf
      pkgs.boost
      pkgs.gperftools
      pkgs.zlib
    ]}:$PKG_CONFIG_PATH
    export LIBRARY_PATH=${pkgs.lib.makeLibraryPath [ pkgs.abseil-cpp pkgs.gperftools pkgs.boost ]}:$LIBRARY_PATH

    pushd gem5/gem5
    BEBOP_IPC_LIB=$BEBOP_IPC_LIB \
    BEBOP_IPC_INCLUDE=$BEBOP_IPC_INCLUDE \
      scons build/RISCV/gem5.opt \
        EXTRAS=$(pwd)/../BebopInOCPU \
        LIBS="absl_log_internal_check_op absl_log_internal_conditions absl_log_internal_message absl_base absl_raw_logging_internal absl_strings absl_throw_delegate absl_string_view absl_spinlock_wait absl_int128 absl_log_severity"
    popd

    pushd gem5/simpoint
    make clean || true
    make
    popd
  '';

  installPhase = ''
    mkdir -p $out/bin
    cp gem5/gem5/build/RISCV/gem5.opt $out/bin/
    mkdir -p $out/share/simpoint
    cp -r gem5/simpoint/* $out/share/simpoint/
  '';

  meta = with pkgs.lib; {
    description = "Bebop-integrated gem5 build";
    homepage = "https://github.com/betrusted-io/buckyball";
    license = licenses.bsd3;
    platforms = platforms.linux;
  };
}