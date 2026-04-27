{ pkgs }:

{
  # Spike build dependencies (Boost disabled in configure)
  dtc = pkgs.dtc;
  autoconf = pkgs.autoconf;
  automake = pkgs.automake;
}
