{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.claude-bar;
in {
  options.programs.claude-bar = {
    enable = mkEnableOption "Claude Bar usage monitor";

    package = mkOption {
      type = types.package;
      default = pkgs.claude-bar;
      defaultText = literalExpression "pkgs.claude-bar";
      description = "The claude-bar package to use.";
    };
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
