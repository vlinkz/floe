{
  nixConfig = {
    extra-substituters = [ "https://cache.floepkgs.org/floe" ];
    extra-trusted-public-keys = [ "floe:EFpHpRyPQiuT+GepNk8DuL+pNB8JlWRiNk6uSAJ3Uuk=" ];
  };

  outputs =
    { self }:
    let
      # Use npins as the source of truth for nixpkgs revision
      sources = import ./npins;
      nixpkgs = sources.nixos-unstable;
      pkgsFor = system: import nixpkgs { inherit system; };
      lib = (pkgsFor "x86_64-linux").lib;

      buildsDir = ./builds;
      appstreamDir = ./appstream;
      buildsEntries = if builtins.pathExists buildsDir then builtins.readDir buildsDir else { };
      appDirs = lib.filterAttrs (_: type: type == "directory") buildsEntries;

      systemsForApp =
        id:
        let
          entries = builtins.readDir (buildsDir + "/${id}");
          isShard = name: type: type == "regular" && lib.hasSuffix ".json" name;
          shards = lib.filterAttrs isShard entries;
        in
        map (n: lib.removeSuffix ".json" n) (builtins.attrNames shards);

      appSystems = lib.unique (builtins.concatMap (id: systemsForApp id) (builtins.attrNames appDirs));

      forAppSystems = f: lib.genAttrs appSystems (system: f system (pkgsFor system));

      builderSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forBuilderSystems = f: lib.genAttrs builderSystems (system: f system (pkgsFor system));
    in
    {
      packages = forAppSystems (
        _system: pkgs:
        let
          apps = import ./nix/packages.nix { inherit pkgs buildsDir; };
          catalog = import ./nix/appstream-data.nix { inherit pkgs appstreamDir; };
        in
        apps // lib.optionalAttrs (catalog != null) { floe-appstream-data = catalog; }
      );

      apps = forBuilderSystems (
        _system: pkgs: {
          floe-builder = {
            type = "app";
            program = "${pkgs.callPackage ./builder { }}/bin/floe-builder";
          };
        }
      );

      devShells = forBuilderSystems (
        _system: pkgs: {
          default = import ./nix/shells/default.nix { inherit pkgs; };
          ci = import ./nix/shells/ci.nix { inherit pkgs; };
        }
      );
    };
}
