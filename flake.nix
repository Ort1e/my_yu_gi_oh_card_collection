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

        pythonPackages = pkgs.python313Packages;

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

            ###############################
            # Python + scientific stack
            ###############################
            pythonPackages.python
            pythonPackages.venvShellHook
            
            autoPatchelfHook


            ##########################
            # Databases
            ##########################
            sqlite
            sqlite-web

            ##########################
            # Node.js
            ##########################
            nodejs
          ];

          #############################################
          # Python venv handling
          #############################################
          venvDir = "./.venv";

          postVenvCreation = ''
            unset SOURCE_DATE_EPOCH

            source .venv/bin/activate

            pip install -r requirements.txt

            autoPatchelf ./.venv
          '';

          postShellHook = ''
            unset SOURCE_DATE_EPOCH
          '';
        };
      }
    );
}
