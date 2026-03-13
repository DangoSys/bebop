# wasm build environment
# Provides: wasm-pack, wasm-bindgen-cli, python3 (for serving)
{ pkgs, rustToolchain }:

{
  buildInputs = [
    (rustToolchain.override { targets = [ "wasm32-unknown-unknown" ]; })
    pkgs.wasm-pack
    pkgs.wasm-bindgen-cli
    pkgs.python3
  ];

  shellHook = ''
    echo "bebop wasm dev environment ready"
    echo "Building wasm..."
    wasm-pack build src/wasm --target web --out-dir web/pkg
    echo ""
    echo "  wasm-pack build src/wasm --target web  - rebuild wasm"
    echo "  python3 -m http.server --directory src/wasm/web 8080 - serve demo"
  '';
}
