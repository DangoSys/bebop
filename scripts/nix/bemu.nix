{ pkgs, base, bebopSrc ? ../.. }:

let
  spikeUrl = "https://github.com/riscv-software-src/riscv-isa-sim.git";
  spikeRev = "591cff16109ced6a21bb2a612a3853b4e9cbd86d";
  spikeSrc = builtins.fetchGit {
    url = spikeUrl;
    rev = spikeRev;
  };
in
rec {
  spikeDrv = pkgs.stdenv.mkDerivation {
    pname = "spike";
    version = spikeRev;
    src = spikeSrc;
    nativeBuildInputs = [
      base.autoconf
      base.automake
      base.libtool
      base.gnumake
      base.pkgConfig
      base.dtc
    ];
    buildInputs = [
      base.gcc
      base.boost
    ];
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild
      mkdir -p build
      cd build
      export BOOST_CPPFLAGS="-I${base.boost}/include"
      export BOOST_LDFLAGS="-L${base.boost}/lib"
      ../configure --prefix="$out" --with-boost-regex=boost_regex
      make -j"$NIX_BUILD_CORES"
      make install
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      true
      runHook postInstall
    '';
  };

  bebopRoccDrv = pkgs.stdenv.mkDerivation {
    pname = "bebop-rocc";
    version = "0.1.0";
    src = bebopSrc;
    nativeBuildInputs = [
      base.cmake
      base.ninja
    ];
    buildInputs = [ spikeDrv ];
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild
      src_dir="$PWD/src/node/spike"
      build_dir="$PWD/build-rocc"
      cmake -G Ninja -S "$src_dir" -B "$build_dir" -DSPIKE_EXE=${spikeDrv}/bin/spike
      ninja -C "$build_dir" bebop_rocc
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      install -Dm755 "$PWD/build-rocc/libbebop_rocc.so" "$out/lib/libbebop_rocc.so"
      runHook postInstall
    '';
  };

  buildInputs = [ spikeDrv bebopRoccDrv ];
}
