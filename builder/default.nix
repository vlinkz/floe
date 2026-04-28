{
  pkgs ? import <nixpkgs> { },
}:
let
  manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
in
pkgs.rustPlatform.buildRustPackage {
  pname = manifest.name;
  version = manifest.version;

  src = pkgs.lib.cleanSource ./.;
  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "libappstream-0.5.0" = "sha256-wbYsyZhcySE2PsLLn7YIt44zfKazVUEYet9YnSuesrc=";
    };
  };

  nativeBuildInputs = with pkgs; [
    makeWrapper
    pkg-config
    rustPlatform.bindgenHook
  ];
  buildInputs = with pkgs; [
    appstream
    glib
  ];

  postInstall = ''
    wrapProgram $out/bin/floe-builder \
      --prefix PATH : ${
        pkgs.lib.makeBinPath [
          pkgs.nix
          pkgs.appstream
        ]
      } \
      --set-default FLOE_APPSTREAMCLI ${pkgs.appstream}/bin/appstreamcli \
      --set-default GDK_PIXBUF_MODULE_FILE \
        ${pkgs.librsvg}/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache
  '';

  meta = {
    mainProgram = "floe-builder";
  };
}
