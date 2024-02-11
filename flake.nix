{
  description = "ircjournal, a lightweight, fast, standalone IRC log viewer for the web, with real-time log ingestion.";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils }: flake-utils.lib.eachDefaultSystem (system:
    let pkgs = import nixpkgs { inherit system; };
    in rec
    {
      packages = rec {
        ircjournal = pkgs.rustPlatform.buildRustPackage {
          pname = "ircjournal";
          version = "local";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          SQLX_OFFLINE = 1;
          nativeBuildInputs = with pkgs; [
            pkg-config
            nodePackages.typescript
            sassc
          ];
          buildInputs = with pkgs; [ openssl ];
        };
        default = ircjournal;
      };
      devShells.default = pkgs.mkShell {
        shellHook = ''
          echo 'To create a test DB, use: export DATABASE_URL=$(pg_tmp -w 0 -d /tmp/ircjournal-pg)'
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
        ];
      };
    });
}
