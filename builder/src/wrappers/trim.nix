# Strip binaries and scrub leaked build-toolchain references.
{
  nixpkgsUrl,
  nixpkgsHash,
  system,
  upstreamPath,
  directRefs,
  mainProgram,
  strip,
  scrubToolchain,
}:
let
  pkgs = import (builtins.fetchTarball {
    url = nixpkgsUrl;
    sha256 = nixpkgsHash;
  }) { inherit system; };

  inherit (pkgs) lib;

  toolchainPatterns = {
    rust = [
      "rust-minimal-"
      "rust-nightly-"
      "rust-beta-"
      "rust-stable-"
      "rustc-"
      "rust-std-"
    ];
  };

  stem =
    path:
    let
      base = baseNameOf path;
      m = builtins.match "[^-]+-(.*)" base;
    in
    if m == null then base else builtins.head m;

  matchesFamily = patterns: path: lib.any (p: lib.hasPrefix p (stem path)) patterns;

  classifyAll = lib.flatten (
    lib.mapAttrsToList (
      family: patterns:
      map (path: { inherit family path; }) (lib.filter (matchesFamily patterns) directRefs)
    ) toolchainPatterns
  );

  detected = if scrubToolchain then classifyAll else [ ];
  scrubs = map (d: d.path) detected;
  scrubFamilies = lib.unique (map (d: d.family) detected);

  withContext =
    path:
    builtins.appendContext path {
      ${path} = {
        path = true;
      };
    };
  upstream = withContext upstreamPath;
  scrubs' = map withContext scrubs;
  scrubList = lib.concatStringsSep " " scrubs';

  ops = map (f: "scrub-toolchain:${f}") scrubFamilies ++ lib.optional strip "strip-binaries";

  stripBlock = lib.optionalString strip ''
    for f in "$out"/bin/* "$out"/lib/*.so* "$out"/lib/lib*.so*; do
      [ -f "$f" ] || continue
      [ -L "$f" ] && continue
      strip --strip-unneeded "$f" 2>/dev/null || true
    done
  '';

  scrubBlock = lib.optionalString (scrubs != [ ]) ''
    for tc in ${scrubList}; do
      for f in "$out"/bin/*; do
        [ -f "$f" ] || continue
        [ -L "$f" ] && continue
        remove-references-to -t "$tc" "$f"
      done
    done
  '';

  smokeBlock = ''
    if [ -x "$out/bin/${mainProgram}" ]; then
      "$out/bin/${mainProgram}" --version > /dev/null 2>&1 || {
        echo "trim smoke test failed for ${mainProgram}" >&2
        exit 1
      }
    fi
  '';
in
pkgs.runCommand "floe-trim"
  {
    nativeBuildInputs = [
      pkgs.removeReferencesTo
      pkgs.binutils
    ];
    disallowedReferences = scrubs';
    passthru = { inherit upstream ops; };
  }
  ''
    cp -a --no-preserve=ownership ${upstream} $out
    chmod -R u+w $out
    ${stripBlock}
    ${scrubBlock}
    ${smokeBlock}
  ''
