{
  description = "service to show pretty colors on the apple touchbar";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {self, nixpkgs, crane}:
  let
    supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
    forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    pkgsFor = forAllSystems (system: import nixpkgs { inherit system; });
  in {
    packages = forAllSystems (system:
      let
        pkgs = pkgsFor.${system};
        craneLib = crane.mkLib pkgs;
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.libinput ];
        };
      in {
        default = craneLib.buildPackage (
          commonArgs
          // {
            cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          }
        );
      }
    );

    devShells = forAllSystems (system:
      let
        pkgs = pkgsFor.${system};
        craneLib = crane.mkLib pkgs;
      in
        craneLib.devShell {}
    );

    nixosModules.better-touchbar = {config, pkgs, lib, ...}: let
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
          
          serviceConfig = {
            ExecStart = "${config'.package}/bin/better-touchbar";
            Restart = "always";
          };

          after = ["systemd-user-sessions.service" "getty@tty1.service" "plymouth-quit.service" "systemd-logind.service"];
          wantedBy = ["multi-user.target"];
        };
      };
    };
  };
}
