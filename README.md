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

## License

MIT
