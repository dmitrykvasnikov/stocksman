# Stocksman

Stocksman is a read-only desktop crypto charting workbench. The first milestone uses public Binance Spot market data, historical candlestick backfill, live updates, multiple independent chart tabs, and safe custom signal visualization.

Trading execution, exchange accounts, API keys, balances, portfolios, alerts, notifications, and automated trading are intentionally out of scope.

## Development status

The repository currently contains the product and architecture documentation. The Tauri application scaffold is the first implementation task in [`PLAN.md`](PLAN.md).

## Required tools

Install these on a clean development machine before building the application:

- Git 2.x or newer.
- Node.js LTS. The exact supported major version will be recorded here when the frontend scaffold is created.
- npm, included with Node.js. Use the repository lockfile and `npm ci` for reproducible installs.
- Rust stable through `rustup`, including the `cargo` toolchain.
- Tauri CLI, installed as the project dependency. Do not rely on a globally installed Tauri CLI.
- A C/C++ linker and build tools required by Rust and Tauri.
- SQLite development support is not required at runtime if the Rust SQLite crate is configured with bundled SQLite. Install the system SQLite CLI only if local database inspection is useful.

## Linux prerequisites

For Debian/Ubuntu-like systems, install the Tauri WebKit and GTK development packages documented for the selected Tauri version, plus:

```sh
sudo apt update
sudo apt install -y \
  build-essential curl wget file libssl-dev \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Package names vary by distribution and release. On Arch Linux, the corresponding development packages are expected to include `base-devel`, `webkit2gtk-4.1`, `openssl`, `appmenu-gtk-module`, `libappindicator-gtk3`, and `librsvg`.

The Linux build must be verified on a clean supported distribution image before release. X11 and Wayland should use platform-neutral Tauri APIs wherever possible.

## Windows prerequisites

Install:

- Microsoft C++ Build Tools with the Desktop development with C++ workload.
- Windows 10 SDK or a newer Windows SDK.
- WebView2 Runtime. It is included on many supported Windows installations, but the packaged application must still be tested on a clean machine.
- Node.js LTS and Rust stable through `rustup`.

The Windows build must be verified in CI and on a clean Windows machine before release.

## Install the project

After the Tauri scaffold is added:

```sh
git clone <repository-url>
cd stocksman
rustup show
node --version
npm --version
npm ci
```

If Rust is not installed:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

## Run in development

Once the application scaffold exists:

```sh
npm run tauri dev
```

The app should start the private Rust/Tokio backend sidecar automatically. The backend must listen on loopback only; no Binance credentials are needed for the public Spot data milestone.

## Build a release package

Once the application scaffold exists:

```sh
npm run tauri build
```

Build artifacts and platform-specific packaging requirements will be documented here as they become available.

## Verification

Before declaring a milestone complete:

```sh
npm run lint
npm test
npm run build
```

The exact scripts are part of the scaffold and must be kept synchronized with this README. Rust checks and tests should also be run from the backend workspace, typically with:

```sh
cargo fmt --check
cargo test
```

## Keeping this file current

Update this README in the same change whenever development requires a new tool, system package, environment variable, build command, supported platform, or version constraint. Document requirements even when they are already installed on the maintainer's machine. Prefer lockfiles and CI configuration as the source of truth for exact versions, and reflect those requirements here in human-readable form.

See [`PLAN.md`](PLAN.md) for implementation phases and [`DECISIONS.md`](DECISIONS.md) for durable product and architecture decisions.
