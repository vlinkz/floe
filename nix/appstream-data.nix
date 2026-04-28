{
  pkgs,
  appstreamDir ? ../appstream,
}:

let
  system = pkgs.stdenv.hostPlatform.system;

  recordPath = appstreamDir + "/${system}.json";

  record =
    if builtins.pathExists recordPath then builtins.fromJSON (builtins.readFile recordPath) else null;
in

if record == null then
  null
else
  let
    src = pkgs.fetchurl {
      name = "floe-appstream-data.tar.gz";
      inherit (record) url hash;
    };
  in
  pkgs.runCommand "floe-appstream-data"
    {
      passthru = {
        inherit (record) generated;
      };
      meta = {
        description = "Combined AppStream catalog for floe-built apps";
        platforms = [ system ];
      };
    }
    ''
      mkdir -p $out
      tar -xzf ${src} -C $out
    ''
