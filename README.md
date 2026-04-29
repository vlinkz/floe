# floe

:warning: WIP :warning:

Prebuilt & self-published Linux app distribution via Nix.

## Use the binary cache

### NixOS

```nix
nix.settings = {
  extra-substituters = [ "https://cache.floepkgs.org/floe" ];
  extra-trusted-public-keys = [ "floe:EFpHpRyPQiuT+GepNk8DuL+pNB8JlWRiNk6uSAJ3Uuk=" ];
};
```

### Standalone Nix

Add to `~/.config/nix/nix.conf` (per-user) or `/etc/nix/nix.conf` (system-wide):

```ini
extra-substituters = https://cache.floepkgs.org/floe
extra-trusted-public-keys = floe:EFpHpRyPQiuT+GepNk8DuL+pNB8JlWRiNk6uSAJ3Uuk=
```

Restart the Nix daemon after editing the system-wide file:

```bash
sudo systemctl restart nix-daemon
```

## Install an app

```bash
nix profile install github:vlinkz/floe#'"com.mitchellh.ghostty"'
nix run github:vlinkz/floe#'"org.prismlauncher.PrismLauncher"'
```

# Todo
- [ ] Sandboxing with `bwrap`
