{
  description = "mfe-player — a plugin-driven, terminal-native music player";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          runtime = with pkgs; [ mpv yt-dlp python3 ffmpeg ];
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "mfe-player";
            version = "0.2.1";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.makeWrapper ];

            # The player shells out to mpv, yt-dlp, python3 and ffmpeg at
            # runtime; wrapping PATH means `nix run` works without installing
            # them globally.
            postInstall = ''
              wrapProgram $out/bin/mfe-player \
                --prefix PATH : ${pkgs.lib.makeBinPath runtime}
              ln -s mfe-player $out/bin/mfe
            '';

            meta = with pkgs.lib; {
              description = "Plugin-driven terminal music player";
              homepage = "https://github.com/omkargs/mfe-player";
              license = licenses.mit;
              mainProgram = "mfe-player";
              platforms = platforms.unix;
            };
          };
        });

      devShells = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [ cargo rustc mpv yt-dlp python3 ffmpeg cava playerctl ];
          };
        });
    };
}
