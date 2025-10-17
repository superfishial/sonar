{
  description = "Sonar";

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

        sonar = pkgs.rustPlatform.buildRustPackage {
          pname = "sonar";
          version = "0.1.0";

          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          meta = with pkgs.lib; {
            description = "Sonar";
            homepage = "https://github.com/yourusername/sonar";
            license = licenses.mit;
            maintainers = [ "Liam Dyer <sonar@liam.super.fish>" ];
          };
        };
      in
      {
        packages = {
          default = sonar;
          sonar = sonar;
        };
      }
    )
    // {
      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        with lib;
        let
          cfg = config.services.sonar;
        in
        {
          options.services.sonar = {
            enable = mkEnableOption "Sonar service";

            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.system}.sonar;
              defaultText = literalExpression "self.packages.\${pkgs.system}.sonar";
            };

            user = mkOption {
              type = types.str;
              default = "sonar";
            };

            group = mkOption {
              type = types.str;
              default = "sonar";
              description = "Group under which sonar runs";
            };

            environmentFile = mkOption {
              type = types.nullOr types.path;
              default = null;
              description = "Environment file (for secrets) to pass to the service";
            };

            extraArgs = mkOption {
              type = types.listOf types.str;
              default = [ ];
              description = "Extra command-line arguments to pass to sonar";
              example = [ "--polling-interval-ms=15000" ];
            };
          };

          config = mkIf cfg.enable {
            systemd.services.sonar = {
              description = "Sonar Service";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              serviceConfig = {
                Type = "simple";
                User = cfg.user;
                Group = cfg.group;
                ExecStart = "${cfg.package}/bin/sonar ${escapeShellArgs cfg.extraArgs}";
                Restart = "on-failure";
                RestartSec = "5s";

                # Security hardening
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectHome = true;
                ReadWritePaths = [ ];

                # Environment
                EnvironmentFile = mkIf (cfg.environmentFile != null) cfg.environmentFile;
              };
            };

            users.users = mkIf (cfg.user == "sonar") {
              sonar = {
                isSystemUser = true;
                group = cfg.group;
                description = "Sonar service user";
              };
            };

            users.groups = mkIf (cfg.group == "sonar") {
              sonar = { };
            };
          };
        };
    };
}
