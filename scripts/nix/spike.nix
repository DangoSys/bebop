# Spike (riscv-isa-sim): build as a pure Nix derivation
{ pkgs }:

let
  spikeUrl = "https://github.com/riscv-software-src/riscv-isa-sim.git";
  spikeRev = "591cff16109ced6a21bb2a612a3853b4e9cbd86d";

  spikeSrc = builtins.fetchGit { url = spikeUrl; rev = spikeRev; };
in
rec {
  spikeDrv =
    pkgs.stdenv.mkDerivation {
      pname = "spike";
      version = spikeRev;
      src = spikeSrc;

      nativeBuildInputs = with pkgs; [
        autoconf
        automake
        libtool
        gnumake
        pkg-config
        dtc
      ];

      buildInputs = with pkgs; [
        gcc
        boost.dev
      ];

      dontConfigure = true;

      buildPhase = ''
        runHook preBuild
        mkdir -p build
        cd build

        export BOOST_CPPFLAGS="-I${pkgs.boost.dev}/include"
        export BOOST_LDFLAGS="-L${pkgs.boost.dev}/lib"

        ../configure --prefix="$out" --with-boost-regex=boost_regex

        make -j"$NIX_BUILD_CORES"
        # Run install inside buildPhase to avoid relying on
        # relative paths across Nix phase working directories.
        make install
        runHook postBuild
      '';

      installPhase = ''
        # `make install` already executed in buildPhase.
        runHook preInstall
        true
      '';
    };

  buildInputs = [ spikeDrv ];
  shellHook = "";
}
