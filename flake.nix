{
  description = "Niri Rune bar";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/f9a75f41be3a74eab55d3a7679d34ad6cc0da977";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "aarch64-linux" "x86_64-linux" "aarch64-darwin" ];
      forSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      linuxSystems = [ "aarch64-linux" "x86_64-linux" ];
      forLinuxSystems = f:
        nixpkgs.lib.genAttrs linuxSystems (system: f nixpkgs.legacyPackages.${system});
    in {
      devShells = forSystems (pkgs: {
        default = pkgs.mkShell ({
          packages = with pkgs; [ cargo rustc rustfmt clippy pkg-config ];
        } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          buildInputs = with pkgs; [ libxkbcommon wayland vulkan-loader libGL ];
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath (with pkgs; [ libxkbcommon wayland vulkan-loader libGL ])}:/run/opengl-driver/lib";
        });
      });

      packages = forLinuxSystems (pkgs:
        let
          snowbar = pkgs.rustPlatform.buildRustPackage {
            pname = "snowbar";
            inherit ((pkgs.lib.importTOML ./Cargo.toml).package) version;

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes = {
                # Every crate from the Rune fork revision shares this hash;
                # importCargoLock deduplicates it by revision.
                "rune-0.0.1" = "sha256-Mj7KQJLV7TCtmTu+P5fbZ0+vp+oXfAiuw2BOHqmaPoQ=";
              };
            };

            nativeBuildInputs = with pkgs; [ pkg-config makeWrapper ];
            # Exercise Snowbar's tests without linking all of the study examples.
            cargoTestFlags = [ "--lib" "--bin" "snowbar" ];

            # Rune loads Wayland, xkbcommon, Vulkan/GL and fontconfig at
            # runtime. The wrapper puts them on the loader path for packaged
            # use outside `nix develop`.
            buildInputs = with pkgs;
              [ libxkbcommon wayland vulkan-loader libGL fontconfig ];

            postFixup = ''
              wrapProgram $out/bin/snowbar \
                --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath (with pkgs; [ libxkbcommon wayland vulkan-loader libGL fontconfig ])}:/run/opengl-driver/lib" \
                --prefix PATH : "${pkgs.lib.makeBinPath [ pkgs.systemd ]}"
            '';

            meta = {
              description = "Minimal niri top bar built with Rune";
              license = pkgs.lib.licenses.mit;
              mainProgram = "snowbar";
              platforms = pkgs.lib.platforms.linux;
            };
          };
        in {
          inherit snowbar;
          default = snowbar;
        });
    };
}
