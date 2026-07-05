{
  description = "apple touchbar on nix real";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;

        buildInputs =
          (with pkgs; [
            pkg-config
          ])
          ++ runtimeLibs;

        runtimeLibs = with pkgs; [
          libinput
        ];

        better-touchbar = craneLib.buildPackage {
          src = craneLib.cleanCargoSource ./.;
          #cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          strictDeps = true; # not sure what this is but seems important

          inherit buildInputs;
        };
      in
      {
        checks = {
          inherit better-touchbar;
        };

        packages.default = better-touchbar;

        # needed?
        /*apps.default = flake-utils.lib.mkApp {
          drv = better-touchbar;
        };*/

        devShells.default = craneLib.devShell {
          inherit buildInputs;
        };
      }
    );
}
