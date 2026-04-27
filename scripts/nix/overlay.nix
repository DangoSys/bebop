final: prev:
{
  base = final.callPackage ./base.nix { };
  riscv = final.callPackage ./riscv.nix { };
  bebop = final.callPackage ./bebop.nix { };
  verilator = (import ./verilator.nix { pkgs = prev; }).verilator;
}
