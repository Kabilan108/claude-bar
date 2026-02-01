{
  description = "Claude Bar - Linux system tray for AI coding assistant usage monitoring";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
          wrapGAppsHook4
        ];

        buildInputs = with pkgs; [
          gtk4
          gtk4-layer-shell
          libadwaita
          glib
          dbus
          openssl
          pango
          gdk-pixbuf
          cairo
          graphene
        ];

        claude-bar = pkgs.rustPlatform.buildRustPackage {
          pname = "claude-bar";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          inherit nativeBuildInputs buildInputs;

          # Tests require fontconfig/GTK which don't work in the Nix sandbox
          doCheck = false;

          postInstall = ''
            # Generate shell completions
            mkdir -p $out/share/bash-completion/completions
            mkdir -p $out/share/zsh/site-functions
            mkdir -p $out/share/fish/vendor_completions.d

            $out/bin/claude-bar completions bash > $out/share/bash-completion/completions/claude-bar
            $out/bin/claude-bar completions zsh > $out/share/zsh/site-functions/_claude-bar
            $out/bin/claude-bar completions fish > $out/share/fish/vendor_completions.d/claude-bar.fish
          '';

          meta = with pkgs.lib; {
            description = "Linux system tray for AI coding assistant usage monitoring";
            homepage = "https://github.com/kabilan/claude-bar";
            license = licenses.mit;
            maintainers = [ ];
          };
        };
      in
      {
        packages = {
          default = claude-bar;
          claude-bar = claude-bar;
        };

        devShells.default = pkgs.mkShell {
          inherit buildInputs;
          nativeBuildInputs = nativeBuildInputs ++ (with pkgs; [
            cargo-watch
            clippy
          ]);

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          # GTK4 requires these for development
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        };

        homeManagerModules.default = import ./nix/hm-module.nix;
        homeManagerModules.claude-bar = self.homeManagerModules.${system}.default;

        nixosModules.default = import ./nix/nixos-module.nix;
        nixosModules.claude-bar = self.nixosModules.${system}.default;
      }
    ) // {
      # Non-system-specific outputs
      homeManagerModules.default = import ./nix/hm-module.nix;
      homeManagerModules.claude-bar = import ./nix/hm-module.nix;

      nixosModules.default = import ./nix/nixos-module.nix;
      nixosModules.claude-bar = import ./nix/nixos-module.nix;

      overlays.default = final: prev: {
        claude-bar = self.packages.${final.system}.claude-bar;
      };
    };
}
