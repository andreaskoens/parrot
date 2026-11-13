# Parrot (Rust rewrite)

Desktop writing app built with egui/eframe. The runnable binary is `parrot-app`; the other crates (`parrot-core`, `-container`, `-markup`, `-style`, `-syntax`, `-export`, `-import`) are libraries it wires together.

`rust-toolchain.toml` pins Rust **1.99.1** with the `x86_64-pc-windows-msvc` target, so Windows is the primary platform.

## Running on Windows (recommended)

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **"Desktop development with C++"** workload (needed for the MSVC linker).
2. Install Rust via [rustup](https://rustup.rs).
3. From PowerShell:

   ```powershell
   cd parrot
   cargo run -p parrot-app --release
   ```

   rustup fetches the pinned toolchain automatically on first build. Drop `--release` for a faster-compiling debug build.

## Running in WSL (via WSLg)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
sudo apt install build-essential pkg-config libxkbcommon-dev libgl1-mesa-dev libwayland-dev libx11-dev
cargo run -p parrot-app
```

This builds a native Linux binary rather than the Windows `.exe`. Building under `/mnt/c` is slow; cloning into `~/` is much faster.

## Other commands

| Command | Purpose |
| --- | --- |
| `cargo test --workspace` | Run all tests |
| `scripts/check.sh` | Project checks |
| `scripts/package_windows.sh` | Build a portable Windows zip into `dist/` |

## Docs

- `docs/manual_smoke_test.md` — click-through checklist once the app is running
- `parrot-app/README.md` — UI conventions
- `PROGRESS.md`, `DECISIONS.md` — status and design decisions
