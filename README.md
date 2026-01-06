<h1>
  <picture>
    <source srcset="https://fonts.gstatic.com/s/e/notoemoji/latest/1faa4/512.webp" type="image/webp">
    <img src="https://fonts.gstatic.com/s/e/notoemoji/latest/1faa4/512.gif" alt="🪤" width="32" height="32">
  </picture>
  Wayscope
</h1>

Profile-based [gamescope](https://github.com/ValveSoftware/gamescope) wrapper for gaming on Linux.

## Why wayscope?

Gamescope can be a guessing game and insanely frustrating to use; variables, CLI flags, and workarounds. And when you finally figure out the commands you have to apply them in too many places and you better not forget them. Wayscope is a tool that helps with this.

- **Environment setup** - Configures RADV, Wayland, Proton, and SDL variables automatically
- **HDR configuration** - Sets `DXVK_HDR`, `ENABLE_HDR_WSI`, `PROTON_ENABLE_HDR` and the required CLI flags
- **HDR workaround** - Automatically applies `DISABLE_HDR_WSI=1` to child processes when using Wayland + WSI + HDR together (a weird wayland quirk)
- **VRR/Adaptive sync** - Enables `--adaptive-sync` based on your monitor's capabilities
- **Resolution & refresh** - Derives `--output-width`, `--output-height`, `--nested-refresh` from your monitor config
- **WSI layer** - Manages `ENABLE_GAMESCOPE_WSI` for proper Vulkan integration
- **Profile switching** - Easily swap between HDR, SDR, performance, etc configs

## Quick Start

```bash
# Initialize config files
wayscope init

# Edit your monitor settings
$EDITOR ~/.config/wayscope/monitors.yaml

# Run a game
wayscope run steam
wayscope run -p hdr heroic
```

## Configuration

Two files in `~/.config/wayscope/`:

**monitors.yaml** - Your displays and their capabilities:
```yaml
monitors:
  main:
    width: 2560
    height: 1440
    refresh: 165
    vrr: true
    hdr: true
    default: true
```

**config.yaml** - Gaming profiles:
```yaml
profiles:
  default:
    useWSI: true

  hdr:
    useHDR: true
    useWSI: true

  performance:
    useHDR: false
    options:
      nested-width: 1920
      nested-height: 1080
      filter: fsr
```

Profile values override monitor defaults. Run `wayscope init` to create a default configuration with all available options.

## Commands

```bash
wayscope init              # Create config files with examples
wayscope run <command>     # Run through gamescope (default profile)
wayscope run -p hdr steam  # Run with specific profile
wayscope list              # List profiles
wayscope show <profile>    # Show resolved settings
wayscope monitors          # List monitors
```

## Installation

```bash
# From source
cargo build --release
cp target/release/wayscope ~/.local/bin/

# With Nix
nix build
```

### Home Manager Module

Wayscope provides a Home Manager module for declarative configuration. Add it to your flake:

```nix
# flake.nix
{
  inputs.wayscope.url = "github:TophC7/wayscope";

  outputs = { wayscope, ... }: {
    homeConfigurations.you = home-manager.lib.homeManagerConfiguration {
      modules = [
        wayscope.homeManagerModules.wayscope
        # your other modules...
      ];
    };
  };
}
```

Then configure profiles and wrappers:

```nix
# home.nix
{ config, osConfig, lib, pkgs, ... }:
{
  programs.wayscope = {
    enable = true;

    # Wayscope derives resolution, refresh, HDR, VRR from this (can be overridden by profile)
    monitors.main = {
      width = 2560;
      height = 1440;
      refreshRate = 165;
      hdr = true;
      vrr = true;
      primary = true;
    };

    # Define reusable profiles
    profiles = {
      default = {
        useHDR = true;
        useWSI = true;
        options.backend = "wayland";
      };

      steam = {
        useHDR = true;
        useWSI = true;
        options = {
          backend = "wayland";
          steam = true;
        };
        environment = {
          STEAM_FORCE_DESKTOPUI_SCALING = "1";
          STEAM_GAMEPADUI = "1";
        };
      };
    };

    # Generate wrapped executables
    wrappers = {
      steam-wayscope = { # You can name this as "steam" to effectively replace the steam command and always use wayscope even in unmodified .desktop files 
        enable = true;
        profile = "steam";
        command = "${lib.getExe osConfig.programs.steam.package} -bigpicture -tenfoot";
      };

      heroic = {
        enable = true;
        profile = "auto-hdr";
        package = pkgs.heroic;
      };
    };
  };
}
```

### play.nix

Wayscope started as `gamescoperun` inside [play.nix](https://github.com/TophC7/play.nix), a NixOS flake I use for my own gaming setup. If you want Steam with Proton-CachyOS, Gamemode, ananicy, LACT for AMD GPUs, etc. already wired up, it might save you some time.


<h2>
  Steam and Backend Limitations
  <picture>
    <source srcset="https://fonts.gstatic.com/s/e/notoemoji/latest/203c_fe0f/512.webp" type="image/webp">
    <img src="https://fonts.gstatic.com/s/e/notoemoji/latest/203c_fe0f/512.gif" alt="‼" width="32" height="32">
  </picture>
</h2>

Steam has specific quirks when running inside gamescope that affect how you can use wayscope:

**The Problem:** Steam does not support `--backend sdl` when passed through game launch options (`%command%`), but it *does* work when Steam itself is launched with that backend.

This creates two distinct HDR workflows:

#### Mode 1: Native HDR (Wayland Backend + WSI)

```yaml
profiles:
  hdr-native:
    useHDR: true
    useWSI: true
    options:
      backend: wayland
```

- Wayscope automatically disables WSI HDR for child processes (`DISABLE_HDR_WSI=1`) so native HDR games can output HDR directly
- **Do NOT use `hdr-itm-enabled: true`** with this mode—it causes a dark/black screen
- Best for games with native HDR support

#### Mode 2: Tone-Mapped HDR (SDL Backend + ITM)

```yaml
profiles:
  hdr-tonemapped:
    useHDR: true
    useWSI: true
    options:
      backend: sdl
      hdr-itm-enabled: true
```

- Forces HDR tone mapping on all content (SDR games get converted to HDR)
- **Must launch Steam itself through wayscope**, not individual games:

```bash
# This works - Steam launched through wayscope
wayscope run -p hdr-tonemapped steam steam://rungameid/3228590

# This does NOT work - wayscope sdl profile in game launch options
wayscope run -p hdr-tonemapped %command% # Steam launch options for a game, NOT a usual terminal command
```

### Recommended Steam Setup

For the best experience, create multiple launch options via desktop actions rather than trying to configure individual games. Here's a NixOS Home Manager example:

```nix
steam = lib.mkDefault {
  name = "Steam";
  comment = "Steam Client";
  exec = "${lib.getExe osConfig.programs.steam.package}";
  icon = "steam";
  type = "Application";
  terminal = false;
  categories = [ "Game" ];
  mimeType = [
    "x-scheme-handler/steam"
    "x-scheme-handler/steamlink"
  ];
  settings = {
    StartupNotify = "true";
    StartupWMClass = "Steam";
    PrefersNonDefaultGPU = "true";
    X-KDE-RunOnDiscreteGpu = "true";
    Keywords = "gaming;";
  };
  actions = {
    hdr-native = {
      name = "Steam Big Picture (Wayscope Default Profile)";
      exec = "${lib.getExe config.programs.wayscope.wrappers.hdr-native.wrappedPackage}"; # Remember to create the wrappers
    };
    hdr-tonemapped = {
      name = "Steam Big Picture (Wayscope Auto HDR Profile)";
      exec = "${lib.getExe config.programs.wayscope.wrappers.hdr-tonemapped.wrappedPackage}";
    };
  };
};
```

### HDR TL;DR

- **Want auto-HDR for everything?** Launch Steam itself through wayscope with the SDL/ITM profile
- **Have a native HDR game?** Use the Wayland profile and set that game's launch options to `wayscope run -p hdr-native %command%`
- **Why not both?** Use desktop actions to launch Steam in different modes as needed

Wayscope detects when it's already inside gamescope and passes through gracefully, so don't worry about launch options conflicting.

## License

MIT
