{
  pkgs ? import (import ../../npins).nixos-unstable { },
}:
pkgs.mkShell {
  packages = with pkgs; [
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer
    jq
    appstream
    librsvg
    pkg-config
    rustPlatform.bindgenHook
    glib
  ];
  RUST_BACKTRACE = "1";
  RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
}
