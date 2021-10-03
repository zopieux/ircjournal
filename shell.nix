let
  rustOverlay = import (builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz");
  pkgs = import <nixpkgs> { overlays = [ rustOverlay ]; };
  rust = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
    extensions = [ "rust-src" ];
    targets = [ "x86_64-unknown-linux-gnu" ];
  });

  # TODO: dart-sass is not packaged :(
  dart-sass-bin = let version = "1.42.1"; in
    pkgs.stdenv.mkDerivation
      rec {
        pname = "dart-sass";
        inherit version;
        system = "x86_64-linux";
        isExecutable = true;
        src = pkgs.fetchurl {
          sha256 = "1rxswaxly1p829zh6sfbif3fpw5shnkmf604mi1x4v11v4ra8f6b";
          url = builtins.concatStringsSep "/" [
            "https://github.com"
            "sass/dart-sass/releases/download"
            "${version}/dart-sass-${version}-linux-x64.tar.gz"
          ];
        };
        phases = "unpackPhase installPhase";
        installPhase = ''
          mkdir -p $out/bin
          cp -r . $out
          ln -s $out/sass $out/bin/sass
        '';
      };

in
pkgs.mkShell {
  buildInputs = with pkgs;
    [
      rust
      postgresql
      openssl # required by sqlx_macros: libssl, libcrypto
      nodePackages.typescript
      dart-sass-bin
    ];

  SQLX_OFFLINE = "true";
  RUST_LOG = "debug";

  shellHook = ''
    cargo install sqlx-cli --no-default-features --features postgres
  '';
}
