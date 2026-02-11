final: prev:

let
  bebopHost = final.callPackage ./bebop-install.nix { };
  spike = final.callPackage ./spike-install.nix { inherit bebopHost; };
  gem5 = final.callPackage ./gem5-install.nix { inherit bebopHost spike; };
in
{
  bebopHost = bebopHost;
  bebopSpike = spike;
  bebopGem5 = gem5;
}