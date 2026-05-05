{
  pkgs ? import (import ../../npins).nixos-unstable { },
}:
let
  floe-builder = pkgs.callPackage ../../builder { };
in
pkgs.mkShellNoCC {
  packages = [
    floe-builder
    pkgs.attic-client
    pkgs.s5cmd
    pkgs.jq
  ];
}
