{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.claude-bar;
  tomlFormat = pkgs.formats.toml { };
in {
  options.services.claude-bar = {
    enable = mkEnableOption "Claude Bar usage monitor";

    package = mkOption {
      type = types.package;
      default = pkgs.claude-bar;
      defaultText = literalExpression "pkgs.claude-bar";
      description = "The claude-bar package to use.";
    };

    settings = mkOption {
      type = tomlFormat.type;
      default = { };
      example = literalExpression ''
        {
          providers = {
            claude = { enabled = true; };
            codex = { enabled = true; };
            merge_icons = true;
          };
          display = {
            show_as_remaining = false;
          };
          notifications = {
            enabled = true;
            threshold = 0.9;
          };
          debug = false;
        }
      '';
      description = ''
        Configuration for claude-bar. See config.example.toml for all options.
      '';
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."claude-bar/config.toml" = mkIf (cfg.settings != { }) {
      source = tomlFormat.generate "claude-bar-config" cfg.settings;
    };

    systemd.user.services.claude-bar = {
      Unit = {
        Description = "Claude Bar usage monitor";
        After = [ "graphical-session-pre.target" ];
        PartOf = [ "graphical-session.target" ];
      };

      Service = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/claude-bar daemon";
        Restart = "on-failure";
        RestartSec = 5;
      };

      Install = {
        WantedBy = [ "graphical-session.target" ];
      };
    };
  };
}
