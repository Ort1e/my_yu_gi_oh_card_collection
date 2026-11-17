{
  description = "Rust dev environment with unified toolchain";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.11";

    rust-overlay.url = "github:oxalica/rust-overlay";

    flake-utils = {
      url = "github:numtide/flake-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:

      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # your full toolchain (rustc, cargo, rust-src, rust-analyzer, etc.)
        fullToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        devShell = pkgs.mkShell rec {

          #############################################
          # ONE RUST TOOLCHAIN FOR EVERYTHING
          #############################################
          packages = with pkgs;[
            #############################################
            # Rust tools (ALL SAME VERSION as toolchain)
            #############################################
            fullToolchain
            cargo-llvm-cov   # test coverage (needs llvm-tools-preview, rust-toolchain.toml)

            ##########################
            # Databases
            ##########################
            sqlite
            sqlite-web

            sqlx-cli

            ##########################
            # Node.js
            ##########################
            nodejs

            ##########################
            # Deployment (deploy/README.md)
            ##########################
            gh               # GitHub: the CI's runs and logs, pull requests, releases
            shellcheck       # checks deploy/**/*.sh
          ];

          postShellHook = ''
            unset SOURCE_DATE_EPOCH
          '';
        };
      }
    );
}
