{
  description = "my yu gi oh collection";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }@inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        pythonPackages = pkgs.python312Packages;
      in
      {
        

        devShell = pkgs.mkShell rec {
          venvDir = "./.venv";
          buildInputs = with pkgs; [
            # python 
            pythonPackages.python
            pythonPackages.venvShellHook
            pythonPackages.django
            pythonPackages.djangorestframework
            pythonPackages.pypdf
            pythonPackages.requests
            pythonPackages.drf-spectacular
            autoPatchelfHook

            # js
            nodejs
            

            # db
            sqlite
            sqlite-web
          ];

          postVenvCreation = ''
            unset SOURCE_DATE_EPOCH
            pip install -r requirements.txt
            autoPatchelf ./venv
          '';

          postShellHook = ''
            unset SOURCE_DATE_EPOCH
           
          '';
        };
      }
    );
}
