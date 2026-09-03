# Stocksman

Stocksman is a read-only desktop crypto charting workbench. The first milestone uses public Binance Spot market data, historical candlestick backfill, live updates, multiple independent chart tabs, and safe custom signal visualization.

Trading execution, exchange accounts, API keys, balances, portfolios, alerts, notifications, and automated trading are intentionally out of scope.

## Development status

The Tauri 2 desktop shell starts and supervises a Rust/Tokio backend companion process over an OS-assigned loopback port. The backend applies versioned SQLite migrations and persists typed user preferences. Provider-neutral market-data contracts define instruments, intervals, candle history, live upserts, and validated canonical candles. An offline mock provider supplies deterministic history and replay streams for the five initial symbols without network access. A Binance Spot adapter fetches public historical klines over HTTPS and normalizes them at the provider boundary. The in-memory candle store orders out-of-order delivery, ignores exact duplicates, applies revisions, and reports missing interval openings. The desktop workbench renders a configurable candlestick and volume chart from deterministic mock history, including provider-backed symbol and timeframe selectors, price and time axes, and OHLCV hover details. Continuous integration verifies Linux and Windows desktop builds. The next pending work is tracked in [`PLAN.md`](PLAN.md).

## Offline market data

The built-in `mock` provider exposes BTCUSDT, ETHUSDT, BNBUSDT, SOLUSDT, and XRPUSDT with fixed 1-minute, 5-minute, 1-hour, and 1-day datasets. Each symbol and interval has 120 generated candles anchored to a fixed UTC timestamp, so repeated test and development runs receive identical values.

Historical queries return timestamp-ordered candles with inclusive time bounds and retain the newest candles when a limit is supplied. Replay subscriptions emit fixture events in their original order at a configurable cadence; custom fixtures can therefore reproduce duplicates, revisions, and out-of-order events for the candle-store work.

The desktop chart requests canonical history from `GET /market-data/candles` on the private loopback backend. The endpoint accepts `provider`, `symbol`, and `interval`, plus optional `start_timestamp`, `end_timestamp`, and `limit` query parameters. The chart tab loads provider-neutral instruments and intervals from `GET /market-data/catalog`, and its symbol and timeframe selectors request the newest 80 candles for the selected series. The SVG chart supports drag or arrow-key panning, wheel or button zooming, viewport reset, and crosshair OHLCV inspection. Tab persistence and multiple independent tabs remain part of the later chart-workbench phase.

The provider-neutral in-memory store accepts historical batches and live upserts. A batch is fully validated before it changes the cache. Snapshots are ordered by UTC opening timestamp, exact duplicate events are ignored, and a later value for the same provider, symbol, interval, and timestamp replaces the earlier revision. An integration test feeds deliberately duplicated and out-of-order mock replay events through this boundary and verifies the resulting ordered series. Gap detection uses the provider's interval definition, including real UTC calendar-month boundaries.

## Binance historical data

The `binance` provider uses Binance's public market-data-only host and `GET /api/v3/klines`; it does not use API keys or account access. Request it through the same private backend endpoint as the mock provider, for example:

```text
GET /market-data/candles?provider=binance&symbol=BTCUSDT&interval=1h&limit=80
```

Optional `start_timestamp` and `end_timestamp` values are forwarded as inclusive millisecond boundaries. The adapter enforces Binance's 1,000-row maximum, preserves UTC opening timestamps, parses the exchange's decimal strings into finite canonical OHLCV values, and determines the `closed` flag from each kline's closing time. It also exposes the configured five-symbol catalog and Binance Spot interval mapping, although switching the desktop chart from the mock provider is a later plan item.

The transport behavior follows Binance's official [Spot REST API documentation](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md#klinecandlestick-data) and [market-data-only endpoint guidance](https://github.com/binance/binance-spot-api-docs/blob/master/faqs/market_data_only.md). Automated tests use a temporary local HTTP server rather than the live service.

Live provider subscriptions use Binance's public market-data-only WebSocket host and raw `<symbol>@kline_<interval>` streams. Incoming open-candle updates and final closed-candle updates are validated against the subscription, normalized to provider-neutral candle upserts, and stop promptly when the subscription is cancelled. REST rows and WebSocket events share the same private normalization path for UTC timestamps, OHLCV values, stream identity, and candle validation, so no Binance payload type crosses the adapter boundary. Reconnect and overlap recovery are tracked as the next Phase 2 task.

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
