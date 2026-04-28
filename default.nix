{
  pkgs ? import (import ./npins).nixos-unstable { },
  buildsDir ? ./builds,
  appstreamDir ? ./appstream,
}:

let
  apps = import ./nix/packages.nix { inherit pkgs buildsDir; };
  catalog = import ./nix/appstream-data.nix { inherit pkgs appstreamDir; };
in
apps // pkgs.lib.optionalAttrs (catalog != null) { floe-appstream-data = catalog; }
