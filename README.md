# Stocksman

Stocksman is a read-only desktop crypto charting workbench. The first milestone uses public Binance Spot market data, historical candlestick backfill, live updates, multiple independent chart tabs, and safe custom signal visualization.

Trading execution, exchange accounts, API keys, balances, portfolios, alerts, notifications, and automated trading are intentionally out of scope.

## Development status

The Tauri 2 desktop shell starts and supervises a Rust/Tokio backend companion process over an OS-assigned loopback port. The backend applies versioned SQLite migrations and persists typed user preferences. Provider-neutral market-data contracts now define instruments, intervals, candle history, live upserts, and validated canonical candles. Continuous integration verifies Linux and Windows desktop builds. The next implementation task in [`PLAN.md`](PLAN.md) is the offline mock/replay provider.

## Required tools

Install these on a clean development machine before building the application:

- Git 2.x or newer.
- Node.js 20 or newer. The scaffold is verified with Node.js 20.14.
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

Install the locked project dependencies:

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

```sh
npm run tauri -- dev
```

This starts the read-only desktop shell and its private backend companion process. The shell reports backend readiness and restarts the process after an unexpected exit; no Binance credentials are needed.

## Local configuration

The backend creates `stocksman.sqlite3` in the platform application-data directory and applies embedded migrations before reporting ready. The initial configuration document contains theme, locale, and time-zone preferences; omitted locale and time zone values mean “use the system setting.”

The private loopback API exposes `GET /configuration` and `PUT /configuration`. Configuration writes are typed, reject unknown or invalid fields, and are committed to SQLite before a successful response is returned. The database contains configuration only; historical market data remains in-memory for the first release.

## Build a release package

```sh
npm run tauri -- build --no-bundle
```

This produces an unbundled native executable. Installer/package generation stays disabled until the packaging work is implemented and verified for the target platforms.

## Continuous integration

The GitHub Actions workflow in [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on pinned Ubuntu 22.04 and Windows Server 2022 images. Both targets install dependencies from `package-lock.json`, run the frontend and Rust checks, and build the unbundled Tauri desktop application. Rust dependencies are resolved from `src-tauri/Cargo.lock`.

The Linux target installs Tauri's WebKitGTK build dependencies before compiling. The Windows target uses the Visual Studio build tools and WebView2 environment supplied by the GitHub-hosted Windows image. The workflow can run for pushes, pull requests, or manually from GitHub Actions.

## Verification

Before declaring a milestone complete:

```sh
npm run lint
npm test
npm run build
```

The exact scripts are part of the scaffold and must be kept synchronized with this README. Rust checks and tests should also be run from the backend workspace, typically with:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## Keeping this file current

Update this README in the same change whenever development requires a new tool, system package, environment variable, build command, supported platform, or version constraint. Document requirements even when they are already installed on the maintainer's machine. Prefer lockfiles and CI configuration as the source of truth for exact versions, and reflect those requirements here in human-readable form.

See [`PLAN.md`](PLAN.md) for implementation phases and [`DECISIONS.md`](DECISIONS.md) for durable product and architecture decisions.
