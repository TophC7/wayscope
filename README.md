<h1>
  <picture>
    <source srcset="https://fonts.gstatic.com/s/e/notoemoji/latest/1faa4/512.webp" type="image/webp">
    <img src="https://fonts.gstatic.com/s/e/notoemoji/latest/1faa4/512.gif" alt="🪤" width="32" height="32">
  </picture>
  Wayscope
</h1>

> Profile-based [gamescope](https://github.com/ValveSoftware/gamescope) wrapper for gaming on Linux.
>
> [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/TophC7/wayscope)

## Why wayscope?

Gamescope can be a guessing game and insanely frustrating to use; variables, CLI flags, and workarounds. And when you finally figure out the commands you have to apply them in too many places and you better not forget them. Wayscope is a tool that helps with this.

- **Environment setup** - Configures RADV, Wayland, Proton, and SDL variables automatically
- **HDR configuration** - Sets `DXVK_HDR`, `PROTON_ENABLE_HDR`, and forces Gamescope HDR support/output
- **Modern HDR WSI** - Keeps the obsolete `ENABLE_HDR_WSI` Vulkan shim out of Gamescope unless explicitly configured
- **VRR/Adaptive sync** - Enables `--adaptive-sync` based on your monitor's capabilities
- **Resolution & refresh** - Derives `--output-width`, `--output-height`, `--nested-refresh` from your monitor config
- **WSI layer** - Applies explicit child-only enable/disable state without loading the layer into Gamescope itself
- **Profile switching** - Easily swap between HDR, SDR, performance, etc configs
- **Skip gamescope** - Use `--skip-gamescope` to apply profile environment setup without the gamescope wrapper
- **Unset variables** - Remove inherited environment variables per-profile for fine-grained control

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
    options: # Any gamescope --flag
      nested-width: 1920
      nested-height: 1080
      filter: fsr
```

Profile values override monitor defaults. Run `wayscope init` to create a default configuration with all available options.

`useWSI` is a profile setting, not a Gamescope command-line option. Keep it beside
`options`. `true` enables the implicit Vulkan layer only for Gamescope's child;
`false` explicitly defeats Gamescope's upstream nested-child auto-enable.

## Commands

```bash
wayscope init                           # Create config files with examples
wayscope run <command>                  # Run through gamescope (default profile)
wayscope run -p hdr steam               # Run with specific profile
wayscope run -s bash                    # Skip gamescope, run command directly with profile env
wayscope run -sp wayland %command%      # Skip gamescope, use profile env with gamemode
wayscope list                           # List profiles
wayscope show <profile>                 # Show resolved settings
wayscope monitors                       # List monitors
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
        # unset = [ "SOME_VARIABLE" ];
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

Wayscope defaults to `pkgs.gamescope` and `pkgs.gamescope-wsi`. Select git or
custom builds explicitly from the consumer's package set:

```nix
programs.wayscope.gamescope = {
  package = pkgs.gamescope-git;
  wsiPackage = pkgs.gamescope-git.wsi;
};
```

The overlay providing those packages should build them from the same `pkgs` as
the host graphics stack. This keeps Gamescope, Mesa/RADV, and glibc compatible.

### play.nix

Wayscope started as `gamescoperun` inside [play.nix](https://github.com/TophC7/play.nix), a NixOS flake I use for my own gaming setup. If you want Steam with Proton-CachyOS, Gamemode, ananicy, LACT for AMD GPUs, etc. already wired up, it might save you some time.

## Advanced Usage

### Skip Gamescope with `--skip-gamescope`

Use the `--skip-gamescope` flag (short form: `-s`) to apply profile environment variables without wrapping your command in gamescope. This is useful for games that run well/better without gamescope but still need some environment setup.

Process environment variables from the profile are applied, including base RADV, Wayland, and HDR variables. Gamescope-only WSI state is intentionally omitted.

### Remove Variables with `unset`

Use the `unset` field in profiles to remove specific environment variables from executed process. This is useful for removing inherited variables that interfere with games.

Example:
```yaml
profiles:
  wayland-native:
    useWSI: true
    unset:
      - DISPLAY # Unset X11 display
```

Then assuming you have a Proton version that supports Wayland natively, you can use this profile to spawn the game in a pure Wayland environment:

```bash
wayscope run -sp wayland-native %command%
```

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

- Wayscope enables Gamescope's WSI layer only for the nested child
- Modern Mesa provides native Vulkan HDR WSI; `ENABLE_HDR_WSI=1` must stay unset
- Wayscope forces HDR support and HDR10 PQ output because Gamescope's nested detection is unreliable
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

There are two main approaches to using wayscope with Steam, each with its own trade-offs:

---

#### Option A: Per-Game Launch Parameters (More Control)

Set up game launch parameters for individual games in Steam the way you normally would, but use wayscope in the launch commands.

**In Steam:** Right-click a game > Properties > Launch Options:

```bash
wayscope run -p hdr %command%
```

**Advantages:**
- Fine-grained control over each game's profile
- Easy to switch profiles per-game
- Steam runs normally outside of gamescope

**Disadvantages:**
- More manual setup for each game
- No support for `--backend` flag (see Mode 2 above)

---

#### Option B: Launch Steam Through Wayscope (Set and Forget)

Launch Steam itself through wayscope so everything runs inside gamescope from the start.

```bash
wayscope run -p hdr-tonemapped steam -bigpicture
```

**Advantages:**
- One consistent experience for all games
- Has support for `--backend` flag
- No need to configure individual game launch options

**Disadvantages:**
- You must restart Steam to change the profile
- Steam itself runs inside gamescope, so theres no window navigation
  - I recommend using Big Picture mode
- Better suited for users that want a more consistent HDR experience

**NixOS Home Manager Example (Option B):**

Create desktop actions to launch Steam in different modes:

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

---

### HDR TL;DR

- **Want auto-HDR for everything?** Use **Option B** and launch Steam itself through wayscope with the SDL/ITM profile
- **Have a native HDR game?** Use **Option A** with a Wayland profile: `wayscope run -p hdr-native %command%`
- **Why not both?** Use desktop actions (Option B) for your default mode, and override specific games with launch options (Option A) when needed

Wayscope detects when it is already inside Gamescope and runs the command directly without re-wrapping or reapplying a profile. Existing Gamescope, WSI, and HDR state remains authoritative.

## License

MIT
