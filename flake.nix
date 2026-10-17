{
  description = "ircjournal, a lightweight, fast, standalone IRC log viewer for the web, with real-time log ingestion.";

  inputs.nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils }:
    let
      eachSystem = flake-utils.lib.eachDefaultSystem (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        rec {
          packages = rec {
            ircjournal = pkgs.rustPlatform.buildRustPackage {
              pname = "ircjournal";
              version = "0.2.0";
              src = ./.;
              cargoLock.lockFile = ./Cargo.lock;
              SQLX_OFFLINE = 1;

              nativeBuildInputs = [
                pkgs.pkg-config
                (pkgs.typescript or pkgs.nodePackages.typescript)
                pkgs.sassc
              ];

              buildInputs = [ pkgs.openssl ];

              RUST_BACKTRACE = 1;
            };
            default = ircjournal;
          };

          devShells.default = pkgs.mkShell {
            shellHook = ''
              echo 'To create a test DB, use: export DATABASE_URL=$(pg_tmp -w 0 -d /tmp/ircjournal-pg) && psql "$DATABASE_URL" -c "CREATE EXTENSION IF NOT EXISTS btree_gin;"'
              echo 'To migrate to be able to use sqlx compile-time validation, use: sqlx migrate run --source ircjournal/migrations'
            '';
            SQLX_OFFLINE = 1;
            DATABASE_URL = "postgresql:///test?host=%2Ftmp%2Fircjournal-pg";
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            inputsFrom = with packages; [ ircjournal ];
            buildInputs = with pkgs; [
              cargo
              rustc
              rustfmt
              ephemeralpg
              clippy
              sqlx-cli
              yarn
              nodejs
            ];
          };
        }
      );
    in
    eachSystem // {
      overlays.default = final: prev: {
        ircjournal = self.packages.${prev.stdenv.hostPlatform.system}.default;
      };
    };
}
