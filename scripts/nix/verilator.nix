{ pkgs }:

{
  verilator = pkgs.verilator.overrideAttrs (_old: {
    version = "5.022";
    src = pkgs.fetchurl {
      url = "https://github.com/verilator/verilator/archive/refs/tags/v5.022.tar.gz";
      hash = "sha256-PC9TOPS2zn4vR6FCQBrN0Yy/TF2gYJJhjW0DbAr+8S0=";
    };
    sourceRoot = "verilator-5.022";
    doCheck = false;
  });

  dramsim2 = pkgs.stdenv.mkDerivation {
    pname = "dramsim2";
    version = "2023-05-10";

    src = pkgs.fetchFromGitHub {
      owner = "umd-memsys";
      repo = "DRAMSim2";
      rev = "753819a8571d24f01e44915093e62857efafb97f";
      hash = "sha256-1wrjxqr937yznmf47l3df029j2m5i6rmabr0rpqpl05z2szkmlka";
    };

    nativeBuildInputs = [ pkgs.gnumake ];

    buildPhase = ''
      make libdramsim.so
    '';

    installPhase = ''
      mkdir -p $out/lib $out/include
      cp libdramsim.so $out/lib/
      cp *.h $out/include/
    '';

    meta = {
      description = "DRAMSim2 memory system simulator";
      homepage = "https://github.com/umd-memsys/DRAMSim2";
    };
  };
}
