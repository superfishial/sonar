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
        pkgs = import nixpkgs { inherit system overlays; };

        sonar = pkgs.rustPlatform.buildRustPackage {
          pname = "sonar";
          version = "1.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config ];

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

          # Default monitor configuration
          defaultMonitorsConfig = {
            cpu = {
              temp = {
                threshold = 80;
                duration = "1m";
              };
              usage = [
                {
                  threshold = 25;
                  duration = "12h";
                }
                {
                  threshold = 75;
                  duration = "30m";
                }
              ];
            };
            disk = {
              health.mount_point = "/";
              usage = {
                mount_point = "/";
                threshold = 0.75;
              };
              scrub = {
                mount_point = "/";
                time_since = "60d";
              };
            };
            memory = {
              critical_threshold = 0.9;
              warn_threshold = 0.75;
              duration = "30m";
            };
          };

          # Generate TOML configuration
          monitorsConfigFile = pkgs.writeText "monitors.toml" (generators.toTOML { } cfg.monitors);
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

            monitors = mkOption {
              type = types.attrs;
              default = defaultMonitorsConfig;
              description = ''
                Monitor configuration in TOML format.
                See monitors.toml for the full structure.
              '';
              example = literalExpression ''
                {
                  cpu = {
                    temp = {
                      threshold = 85;
                      duration = "2m";
                    };
                    usage = [
                      {
                        threshold = 30;
                        duration = "6h";
                      }
                    ];
                  };
                  disk = {
                    health = [
                      { mount_point = "/"; }
                      { mount_point = "/data/hdd"; }
                    ];
                    usage = [
                      {
                        mount_point = "/";
                        threshold = 0.8;
                      }
                    ];
                  };
                  memory = {
                    critical_threshold = 0.95;
                    warn_threshold = 0.8;
                    duration = "15m";
                  };
                  nixpkgs = {
                    path = "/home/user/nixfiles/flake.lock";
                    max_age = "30d";
                  };
                  systemd = {
                    service_name = "restic-backups-primary";
                    max_time_since = "1d";
                  };
                }
              '';
            };

            extraArgs = mkOption {
              type = types.listOf types.str;
              default = [ ];
              description = "Extra command-line arguments to pass to sonar";
              example = [ "--polling-interval-ms=15001" ];
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
                ExecStart = "${cfg.package}/bin/sonar --monitors-config-path ${monitorsConfigFile} ${escapeShellArgs cfg.extraArgs}";
                EnvironmentFile = mkIf (cfg.environmentFile != null) cfg.environmentFile;
                Restart = "on-failure";
                RestartSec = "6s";

                # Security hardening
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectHome = true;
                ReadWritePaths = [ ];
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
