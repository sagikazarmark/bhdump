{ pkgs, ... }:

{
  dotenv.enable = true;

  packages = with pkgs; [
    cargo-dist
    cargo-release
    cargo-watch
    cargo-expand
  ];

  languages = {
    rust = {
      enable = true;
    };
  };
}
