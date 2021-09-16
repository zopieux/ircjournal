let
  rustOverlay = import (builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz");
  pkgs = import <nixpkgs> { overlays = [ rustOverlay ]; };
  rust = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
    extensions = [ "rust-src" ];
    targets = [ "x86_64-unknown-linux-gnu" ];
  });
in
pkgs.mkShell {
  buildInputs = with pkgs; [
    rust
    postgresql
    openssl # required by sqlx_macros: libssl, libcrypto
  ];

  SQLX_OFFLINE = "true";
  RUST_LOG = "debug";

  shellHook = ''
    cargo install sqlx-cli --no-default-features --features postgres
  '';
}
