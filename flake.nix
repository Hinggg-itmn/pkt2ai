{
  description = "pkt2ai - Packet Tracer decoder, topology CLI and TUI";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "pkt2ai";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = "Decode Packet Tracer files and inspect topology in a TUI";
              mainProgram = "pkt2ai";
              license = pkgs.lib.licenses.mit;
            };
          };
        });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/pkt2ai";
        };
      });

      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [ cargo rustc clippy rustfmt rust-analyzer ];
            RUST_BACKTRACE = "1";
          };
        });
    };
}
