# bebop-next
A buckyball emulator written in Rust


### Quick start

1. Setup the repo

```
git clone https://github.com/DangoSys/bebop.git
cd bebop
git checkout next
```

2. Build the simulator

```
cd bebop
nix build
```

### Run in CLI

```
cd bebop
nix develop
bebop workload
bebop spike-test
```

### Run in GUI

```
cd bebop
nix develop .#tauri
pnpm --dir src/tauri tauri dev
```

### Run in web

```
cd bebop
nix develop .#wasm
python3 -m http.server 8080 --directory src/wasm/web
```

Then access this link in your web browser http://localhost:8080.
