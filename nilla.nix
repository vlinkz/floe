let
  pins = import ./npins;
  nilla = import pins.nilla;

  buildsDir = ./builds;
  appstreamDir = ./appstream;

  appDirs =
    let
      entries = if builtins.pathExists buildsDir then builtins.readDir buildsDir else { };
    in
    builtins.attrNames (
      builtins.foldl' (acc: n: if entries.${n} == "directory" then acc // { ${n} = null; } else acc) { } (
        builtins.attrNames entries
      )
    );

  appSystems =
    id:
    let
      entries = builtins.readDir (buildsDir + "/${id}");
      isShard = n: builtins.match "(.+)\\.json" n != null;
      shardSystem = n: builtins.head (builtins.match "(.+)\\.json" n);
    in
    map shardSystem (builtins.filter isShard (builtins.attrNames entries));

  unique =
    xs:
    let
      go = acc: x: if builtins.elem x acc then acc else acc ++ [ x ];
    in
    builtins.foldl' go [ ] xs;

  allSystems = unique (builtins.concatMap appSystems appDirs);

  catalogSystems =
    let
      entries = if builtins.pathExists appstreamDir then builtins.readDir appstreamDir else { };
      isShard = n: builtins.match "(.+)\\.json" n != null;
      shardName = n: builtins.head (builtins.match "(.+)\\.json" n);
    in
    map shardName (builtins.filter isShard (builtins.attrNames entries));

  packageEntry = id: {
    systems = appSystems id;
    builder = "nixpkgs";
    package = { pkgs }: (import ./nix/packages.nix { inherit pkgs buildsDir; }).${id};
    settings.args = { };
  };

  appstreamEntry = {
    systems = catalogSystems;
    builder = "nixpkgs";
    package = { pkgs }: import ./nix/appstream-data.nix { inherit pkgs appstreamDir; };
    settings.args = { };
  };

  appPackages = builtins.listToAttrs (
    map (id: {
      name = id;
      value = packageEntry id;
    }) appDirs
  );

  packages =
    appPackages // (if catalogSystems == [ ] then { } else { floe-appstream-data = appstreamEntry; });

  builderSystems = [
    "x86_64-linux"
    "aarch64-linux"
  ];

  shellEntry = file: {
    systems = builderSystems;
    builder = "nixpkgs";
    shell = { pkgs }: import file { inherit pkgs; };
    settings.args = { };
  };

  shells = {
    default = shellEntry ./nix/shells/default.nix;
    ci = shellEntry ./nix/shells/ci.nix;
  };

  inputSystems = unique (allSystems ++ builderSystems);
in
nilla.create {
  config = {
    inputs.nixpkgs = {
      src = pins.nixos-unstable;
      settings.systems = inputSystems;
    };

    inherit packages shells;
  };
}
