{
  description = "Dev environment for myapp (axum + sqlx/SQLite backend, Svelte frontend)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        fullToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            fullToolchain

            # sqlx needs the sqlite lib + pkg-config at build time
            pkgs.sqlite
            pkgs.pkg-config
            pkgs.openssl

            # for running migrations / offline query cache outside the app
            pkgs.sqlx-cli

            # frontend (Svelte + Vite)
            pkgs.nodejs_20

            # handy extras
            pkgs.cargo-watch
            pkgs.sqlite-interactive
          ];

          shellHook = ''
            echo "myapp dev shell ready"
            echo "  backend:  cargo run          (or: cargo watch -x run)"
            echo "  frontend: cd frontend && npm install && npm run dev"
          '';
        };
      });
}
