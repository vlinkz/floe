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
    pkgs.awscli2
    pkgs.jq
  ];
}
