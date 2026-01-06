{
  description = "wayscope - Profile-based gamescope wrapper for gaming on Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Use stable Rust toolchain
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
          ];
        };

        # Native build inputs
        nativeBuildInputs = with pkgs; [
          pkg-config
          rustToolchain
        ];

      in
      {
        # Development shell
        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;

          shellHook = ''
            echo "wayscope development shell"
            echo "Rust: $(rustc --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build        - Build the project"
            echo "  cargo build -r     - Build release binary"
            echo "  cargo clippy       - Run linter"
            echo "  cargo fmt          - Format code"
            echo "  cargo test         - Run tests"
            echo ""
          '';
        };

        # Package definition
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "wayscope";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit nativeBuildInputs;

          meta = with pkgs.lib; {
            description = "Profile-based gamescope wrapper for gaming on Linux";
            homepage = "https://github.com/tophc7/wayscope";
            license = licenses.mit;
            maintainers = [ "tophc7" ];
            platforms = platforms.linux;
            mainProgram = "wayscope";
          };
        };

        # Convenient alias
        packages.wayscope = self.packages.${system}.default;
      }
    );
}
