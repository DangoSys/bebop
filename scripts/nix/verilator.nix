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
}
