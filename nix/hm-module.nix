# Home Manager module for wayscope
# Profile-based gamescope wrapper for gaming on Linux
#
# Can integrate with mix.nix monitors (config.monitors) or define its own.
# Provides gamescope-git and gamescope-wsi from chaotic-nyx without requiring
# users to add chaotic to their flake inputs.
{
  config,
  lib,
  pkgs,
  wayscope,
  ...
}:
let
  cfg = config.programs.wayscope;

  # Gamescope packages from chaotic-nyx (provided by wayscope flake)
  gamescopePackages =
    if cfg.useGit then
      {
        gamescope = wayscope.gamescopePackages.gamescope-git;
        gamescope-wsi = wayscope.gamescopePackages.gamescope-wsi-git;
      }
    else
      {
        gamescope = pkgs.gamescope;
        gamescope-wsi = pkgs.gamescope-wsi or null;
      };

  # Binary path based on useGit setting
  defaultBinary = lib.getExe gamescopePackages.gamescope;

  # Check if mix.nix monitors are available
  hasSystemMonitors = config ? monitors && config.monitors != [ ];

  # Convert mix.nix monitor list to wayscope attrset format
  # mix.nix: [{ name, primary, width, height, refreshRate, vrr, hdr, ... }]
  # wayscope: { name = { width, height, refreshRate, vrr, hdr, primary }; }
  # Field names now match - just convert list to attrset
  systemMonitorsToWayscope = lib.listToAttrs (
    map (mon: {
      name = mon.name;
      value = lib.filterAttrs (_: v: v != null) {
        inherit (mon)
          width
          height
          vrr
          hdr
          ;
        refreshRate =
          if builtins.isFloat mon.refreshRate then builtins.floor mon.refreshRate else mon.refreshRate;
        primary = if mon.primary then true else null;
      };
    }) (lib.filter (m: m.enabled) config.monitors)
  );

  # Use system monitors if enabled and available, otherwise use inline config
  effectiveMonitors =
    if cfg.useSystemMonitors && hasSystemMonitors then systemMonitorsToWayscope else cfg.monitors;

  # Use nixpkgs YAML format for proper YAML generation
  yamlFormat = pkgs.formats.yaml { };

  # Generate monitors.yaml content
  monitorsConfig = {
    monitors = lib.mapAttrs (
      _: mon:
      lib.filterAttrs (_: v: v != null) {
        inherit (mon)
          width
          height
          refreshRate
          vrr
          hdr
          ;
        primary = if mon.primary or false then true else null;
      }
    ) effectiveMonitors;
  };

  # Generate config.yaml content
  profilesConfig = {
    profiles = lib.mapAttrs (
      _: prof:
      lib.filterAttrs (_: v: v != null && v != { }) {
        inherit (prof)
          monitor
          useHDR
          useWSI
          ;
        # Convert package to binary path for YAML
        # Nix uses "package" (types.package), YAML uses "binary" (path string)
        binary = if prof.package != null then lib.getExe prof.package else defaultBinary;
        options = if prof.options == { } then null else prof.options;
        environment = if prof.environment == { } then null else prof.environment;
      }
    ) cfg.profiles;
  };

  # Create a wrapper script for an application
  mkWrapper =
    name: wrapperCfg:
    let
      profileArg = lib.optionalString (wrapperCfg.profile != null) "-p ${wrapperCfg.profile}";
      command = if wrapperCfg.command != null then wrapperCfg.command else lib.getExe wrapperCfg.package;
    in
    pkgs.writeShellScriptBin name ''
      exec ${lib.getExe cfg.package} run ${profileArg} ${command} "$@"
    '';

  # Type for gamescope options (matches OptionValue enum in Rust)
  optionValueType =
    with lib.types;
    oneOf [
      bool
      int
      str
    ];
in
{
  options.programs.wayscope = {
    enable = lib.mkEnableOption "wayscope, a profile-based gamescope wrapper";

    package = lib.mkOption {
      type = lib.types.package;
      default = wayscope.packages.default;
      defaultText = lib.literalExpression "wayscope.packages.default";
      description = "The wayscope package to use.";
    };

    useGit = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Use git versions of gamescope from chaotic-nyx for latest features.
        When true, uses gamescope_git and gamescope-wsi_git.
        When false, uses stable gamescope from nixpkgs.
      '';
    };

    # =========================================================================
    # Monitor Configuration
    # =========================================================================
    useSystemMonitors = lib.mkOption {
      type = lib.types.bool;
      default = hasSystemMonitors;
      description = ''
        Use monitors from config.monitors (e.g., from mix.nix) instead of
        defining them inline. Automatically enabled if config.monitors exists.

        When enabled, monitors are read from config.monitors and converted
        from a list to an attrset keyed by name. Only enabled monitors are included.
        Field names match mix.nix format (primary, refreshRate).
      '';
    };

    monitors = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.submodule {
          options = {
            width = lib.mkOption {
              type = lib.types.int;
              example = 2560;
              description = "Native horizontal resolution.";
            };

            height = lib.mkOption {
              type = lib.types.int;
              example = 1440;
              description = "Native vertical resolution.";
            };

            refreshRate = lib.mkOption {
              type = lib.types.int;
              example = 165;
              description = "Refresh rate in Hz.";
            };

            vrr = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = "Whether the monitor supports VRR (FreeSync/G-Sync).";
            };

            hdr = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = "Whether the monitor supports HDR.";
            };

            primary = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = "Use this monitor when profile doesn't specify one.";
            };
          };
        }
      );
      default = { };
      example = lib.literalExpression ''
        {
          main = {
            width = 2560;
            height = 1440;
            refreshRate = 165;
            vrr = true;
            hdr = true;
            primary = true;
          };
          tv = {
            width = 3840;
            height = 2160;
            refreshRate = 120;
            hdr = true;
          };
        }
      '';
      description = "Monitor definitions with their hardware capabilities. Field names match mix.nix format.";
    };

    # =========================================================================
    # Profile Configuration
    # =========================================================================
    profiles = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.submodule {
          options = {
            monitor = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "main";
              description = "Which monitor to use. If null, uses the default monitor.";
            };

            package = lib.mkOption {
              type = lib.types.nullOr lib.types.package;
              default = null;
              example = lib.literalExpression "pkgs.gamescope";
              description = ''
                Gamescope package to use for this profile.
                If null (default), uses gamescope from useGit setting (gamescope_git or stable).
              '';
            };

            useHDR = lib.mkOption {
              type = lib.types.nullOr lib.types.bool;
              default = null;
              description = "Enable HDR output. If null, uses monitor's HDR capability.";
            };

            useWSI = lib.mkOption {
              type = lib.types.nullOr lib.types.bool;
              default = null;
              description = "Enable Gamescope WSI layer. Defaults to true.";
            };

            options = lib.mkOption {
              type = lib.types.attrsOf optionValueType;
              default = { };
              example = {
                backend = "sdl";
                fullscreen = true;
                nested-width = 1920;
                nested-height = 1080;
                filter = "fsr";
              };
              description = ''
                Gamescope command-line options.
                These override the defaults derived from monitor config.
              '';
            };

            environment = lib.mkOption {
              type = lib.types.attrsOf lib.types.str;
              default = { };
              example = {
                MANGOHUD = "1";
                DXVK_ASYNC = "1";
              };
              description = "Additional environment variables for games using this profile.";
            };
          };
        }
      );
      default = { };
      example = lib.literalExpression ''
        {
          default = {
            useWSI = true;
          };
          hdr = {
            useHDR = true;
            useWSI = true;
          };
          performance = {
            useHDR = false;
            options = {
              nested-width = 1920;
              nested-height = 1080;
              filter = "fsr";
            };
          };
        }
      '';
      description = "Gaming profile definitions.";
    };

    # =========================================================================
    # Wrapper Configuration
    # =========================================================================
    wrappers = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.submodule (
          { name, config, ... }:
          {
            options = {
              enable = lib.mkEnableOption "this wrapper";

              profile = lib.mkOption {
                type = lib.types.nullOr lib.types.str;
                default = null;
                example = "hdr";
                description = ''
                  Profile to use for this wrapper.
                  If null, uses the default profile.
                '';
              };

              package = lib.mkOption {
                type = lib.types.nullOr lib.types.package;
                default = null;
                description = ''
                  The package to wrap.
                  Used with lib.getExe if command is not specified.
                '';
              };

              command = lib.mkOption {
                type = lib.types.nullOr lib.types.str;
                default = null;
                example = "steam -tenfoot -bigpicture";
                description = ''
                  The exact command to execute.
                  If specified, takes precedence over package.
                  Can include arguments and flags.
                '';
              };

              # Readonly: The generated wrapper package
              wrappedPackage = lib.mkOption {
                type = lib.types.package;
                readOnly = true;
                default = mkWrapper name config;
                description = "The generated wrapper package. Use this to reference the wrapper in other places.";
              };
            };
          }
        )
      );
      default = { };
      example = lib.literalExpression ''
        {
          steam = {
            enable = true;
            profile = "hdr";
            command = "steam -tenfoot -bigpicture";
          };
          heroic = {
            enable = true;
            profile = "default";
            package = pkgs.heroic;
          };
        }
      '';
      description = ''
        Application wrappers that run through wayscope.
        Creates executables that call `wayscope run -p <profile> <command>`.
        These can override existing binaries in your PATH.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # Add wayscope, gamescope, and wrapper packages to home
    home.packages =
      [
        cfg.package
        gamescopePackages.gamescope
      ]
      ++ lib.optional (gamescopePackages.gamescope-wsi != null) gamescopePackages.gamescope-wsi
      ++ lib.mapAttrsToList (_: w: w.wrappedPackage) (lib.filterAttrs (_: w: w.enable) cfg.wrappers);

    # Generate config files
    xdg.configFile = {
      "wayscope/monitors.yaml" = lib.mkIf (effectiveMonitors != { }) {
        source = yamlFormat.generate "monitors.yaml" monitorsConfig;
      };

      "wayscope/config.yaml" = lib.mkIf (cfg.profiles != { }) {
        source = yamlFormat.generate "config.yaml" profilesConfig;
      };
    };

    # Assertions
    assertions =
      # Ensure exactly one primary monitor (only check if not using system monitors)
      [
        {
          assertion =
            cfg.useSystemMonitors
            || effectiveMonitors == { }
            || (lib.count (m: m.primary or false) (lib.attrValues effectiveMonitors)) == 1;
          message = "programs.wayscope.monitors: Exactly one monitor must have 'primary = true'.";
        }
      ]
      # Ensure wrappers have either command or package
      ++ lib.mapAttrsToList (name: w: {
        assertion = !w.enable || (w.command != null || w.package != null);
        message = "programs.wayscope.wrappers.${name}: Either 'command' or 'package' must be specified when enabled.";
      }) cfg.wrappers
      # Ensure wrapper profiles exist
      ++ lib.mapAttrsToList (name: w: {
        assertion = !w.enable || w.profile == null || cfg.profiles ? ${w.profile};
        message = "programs.wayscope.wrappers.${name}: Profile '${w.profile}' does not exist in programs.wayscope.profiles.";
      }) cfg.wrappers;
  };
}
