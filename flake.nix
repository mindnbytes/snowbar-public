{
  description = "Development environment for the niri Rune bar";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/f9a75f41be3a74eab55d3a7679d34ad6cc0da977";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "aarch64-linux" "x86_64-linux" "aarch64-darwin" ];
      forSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      devShells = forSystems (pkgs: {
        default = pkgs.mkShell ({
          packages = with pkgs; [ cargo rustc rustfmt clippy pkg-config ];
        } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          buildInputs = with pkgs; [ libxkbcommon wayland vulkan-loader libGL ];
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath (with pkgs; [ libxkbcommon wayland vulkan-loader libGL ])}:/run/opengl-driver/lib";
        });
      });
    };
}
