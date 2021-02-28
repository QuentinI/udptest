{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = inputs@{ nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in rec {
        devShell = pkgs.mkShell rec {
          buildInputs = [
            pkgs.expat
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.xorg.libX11
            pkgs.xorg.libxcb
            pkgs.freetype
            pkgs.sqlite.dev
            pkgs.cmake
            pkgs.pkg-config
            pkgs.libGL
          ];

          LD_LIBRARY_PATH = builtins.foldl'
            (a: b: "${a}:${b}/lib") "${pkgs.vulkan-loader}/lib" buildInputs;
        };
      });
}
