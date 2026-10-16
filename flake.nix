{
  description = "pkt2ai - giai ma Cisco Packet Tracer .pkt sang XML/JSON cho AI doc";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in
    {
      # nix build / nix run: cai thanh lenh `pkt2ai` (chi can python3, khong co dependency ngoai)
      packages = forAllSystems (system: pkgs: rec {
        pkt2ai = pkgs.stdenv.mkDerivation {
          pname = "pkt2ai";
          version = "0.1.0";
          src = self;
          buildInputs = [ pkgs.python3 ];
          dontBuild = true;
          installPhase = ''
            install -Dm755 pkt2ai.py $out/bin/pkt2ai
            patchShebangs $out/bin/pkt2ai
          '';
          meta.mainProgram = "pkt2ai";
        };
        default = pkt2ai;
      });

      apps = forAllSystems (system: pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${system}.pkt2ai}/bin/pkt2ai";
        };
      });

      # nix develop: co san python3 va lenh `pkt2ai`
      devShells = forAllSystems (system: pkgs: {
        default = pkgs.mkShell {
          packages = [
            pkgs.python3
            self.packages.${system}.pkt2ai
          ];
          shellHook = ''
            echo "pkt2ai san sang: pkt2ai summary lab.pkt --format md"
          '';
        };
      });
    };
}
