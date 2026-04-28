{
  pkgs,
  buildsDir ? ../builds,
}:

let
  inherit (pkgs) lib;
  system = pkgs.stdenv.hostPlatform.system;

  storePath =
    path:
    builtins.appendContext path {
      ${path} = {
        path = true;
      };
    };

  buildsEntries = if builtins.pathExists buildsDir then builtins.readDir buildsDir else { };
  appDirs = lib.filterAttrs (_: type: type == "directory") buildsEntries;

  loadShard =
    id:
    let
      path = buildsDir + "/${id}/${system}.json";
    in
    if builtins.pathExists path then builtins.fromJSON (builtins.readFile path) else null;

  buildable = lib.filterAttrs (_: v: v != null) (builtins.mapAttrs (id: _: loadShard id) appDirs);

  resolveLicense =
    spdx:
    let
      direct = lib.filter (l: (l.spdxId or null) == spdx) (lib.attrValues lib.licenses);
    in
    if direct != [ ] then lib.head direct else spdx;

  licenseIsUnfree = license: builtins.isAttrs license && (license.free or true) == false;

  mkApp =
    id: shard:
    let
      closure = storePath shard.storePath;
      md = shard.metadata or { };
      license = if md ? license then resolveLicense md.license else null;

      siblingShards = lib.filterAttrs (name: type: type == "regular" && lib.hasSuffix ".json" name) (
        builtins.readDir (buildsDir + "/${id}")
      );
      platforms = map (n: lib.removeSuffix ".json" n) (builtins.attrNames siblingShards);

      meta = {
        mainProgram = shard.mainProgram;
        inherit platforms;
        unfree = (shard.unfree or false) || licenseIsUnfree license;
      }
      // lib.optionalAttrs (md ? summary) { description = md.summary; }
      // lib.optionalAttrs (md ? longDescription) { longDescription = md.longDescription; }
      // lib.optionalAttrs (md ? homepage) { homepage = md.homepage; }
      // lib.optionalAttrs (license != null) { inherit license; };
    in
    pkgs.runCommand id
      {
        inherit meta;
        version = shard.version;
      }
      ''
        ln -s ${closure} $out
      '';

in
builtins.mapAttrs mkApp buildable
