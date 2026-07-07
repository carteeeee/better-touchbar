{
  description = "service to show pretty colors on the apple touchbar";

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

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true; # not sure what this is but seems important
          nativeBuildInputs = [pkgs.pkg-config];
          buildInputs = [pkgs.libinput];
        };

        better-touchbar = craneLib.buildPackage (
          commonArgs
          // {
            cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          }
        );
      in
      {
        checks = {
          inherit better-touchbar;
        };

        packages.default = better-touchbar;

        nixosModules.default = {config, pkgs, lib, ...}: let
          inherit (lib) mkEnableOption mkIf mkOption;
          inherit (lib.types) package;
          config' = config.services.better-touchbar;
        in {
          options = {
            services.better-touchbar = {
              enable = mkEnableOption "whether to enable the better-touchbar service";
              package = mkOption {
                type = package;
                default = self.packages.${pkgs.system}.default;
                description = "package for better-touchbar";
              };
            };
          };
          
          config = mkIf config'.enable {
            systemd.services."better-touchbar" = {
              enable = true;
              description = "pretty touchbar";
              unitConfig = {
                Type = "simple";
              };
              
              serviceConfig = {
                ExecStart = "${config'.package}/bin/better-touchbar";
              };

              after = ["systemd-user-sessions.service" "getty@tty1.service" "plymouth-quit.service" "systemd-logind.service"];
            };
          };
        };

        devShells.default = craneLib.devShell {};
      }
    );
}
