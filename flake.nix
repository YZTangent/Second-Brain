{
  description = "Second Brain v2: Headless Daemon, CLI, and MCP bridge";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      rec {
        packages.default = packages.second-brain;

        packages.second-brain = pkgs.rustPlatform.buildRustPackage {
          pname = "second-brain";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeCheckInputs = [ pkgs.git ];
        };

        packages.braind = packages.second-brain;
        packages.brainctl = packages.second-brain;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
            git
          ];
        };
      });
}
