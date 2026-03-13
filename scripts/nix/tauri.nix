# Tauri 2 + Vite + React development environment
# All tools provided by Nix — no host dependencies required
{ pkgs, rustToolchain }:

let
  # Tauri on Linux needs these system libs
  linuxDeps = pkgs.lib.optionals pkgs.stdenv.isLinux (with pkgs; [
    webkitgtk_4_1
    gtk3
    libsoup_3
    openssl
    glib
    pango
    gdk-pixbuf
    atk
    cairo
    librsvg
    pkg-config
  ]);
in
{
  buildInputs = [
    rustToolchain
    pkgs.nodejs_22
    pkgs.nodePackages.pnpm
    pkgs.cargo-tauri
  ] ++ linuxDeps;

  shellHook = ''
    echo "bebop tauri dev environment ready"
    echo "Installing frontend dependencies..."
    pnpm --dir src/tauri install --frozen-lockfile 2>/dev/null || pnpm --dir src/tauri install
    echo ""
    echo "  pnpm --dir src/tauri tauri dev       - start dev server + window"
    echo "  pnpm --dir src/tauri tauri build     - build release app"
    echo ""
    echo "  cargo run --features gui -- gui      - open GUI window from CLI"
    echo "  cargo build --features gui           - build with GUI support"
  '';
}
