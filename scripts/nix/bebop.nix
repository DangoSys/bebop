{ pkgs, bebopSrc ? ../.. }:

pkgs.rustPlatform.buildRustPackage {
  pname = "bebop";
  version = "0.1.0";
  src = bebopSrc;
  cargoLock.lockFile = "${bebopSrc}/Cargo.lock";
  cargoBuildFlags = [ "--package" "bebop" ];
  cargoTestFlags = [ "--package" "bebop" ];
  nativeBuildInputs = [ ];
}
