{ pkgs }:

{
  buildInputs = [
    pkgs.dtc
    pkgs.protobuf
    pkgs.git
  ];
}
