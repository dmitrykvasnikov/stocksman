# Project Decisions

This file records durable product and architecture decisions. New entries should include the decision, rationale, and date when useful.

## D001 — Read-only first milestone

The first milestone is limited to Binance Spot public market data, historical backfill, live candlesticks, multiple chart tabs, and safe custom signal visualization.

Out of scope: accounts, API keys, balances, portfolios, alerts, notifications, order placement, and automated trading.

## D002 — Desktop shape and stack

Use Tauri 2 with React/TypeScript for the desktop shell and Rust/Tokio for the local backend/core. The app must remain portable to Windows and Linux.

## D003 — Local backend sidecar

Tauri starts and supervises a private backend sidecar over loopback. The backend owns provider connectivity and domain logic. It is not exposed to the local network. This keeps deployment local while preserving a clean path to hosted deployment.

The desktop executable re-launches itself in a dedicated backend mode instead of shipping a second executable. The child process binds an OS-assigned `127.0.0.1` port, reports readiness to its parent, and is restarted after unexpected termination. This preserves process isolation while keeping Linux and Windows packaging simple.

## D004 — Persistence

Use SQLite for signals, chains, enabled states, tab configuration, and user preferences. Do not persist historical market data in the first release; cache it in memory and fetch it on demand.

Keep the database in the platform application-data directory. The backend applies ordered, embedded SQL migrations before it reports ready and records each applied schema version. Store the evolving user-preference shape as validated, typed JSON in a singleton row so new preferences can be added without exposing provider-specific types to the persistence boundary.

## D005 — Provider boundary

Binance-specific transport and types stay inside the Binance adapter. The chart and signal engine consume only canonical candles and provider-neutral interfaces.

## D006 — Initial market scope

Start with BTCUSDT, ETHUSDT, BNBUSDT, SOLUSDT, and XRPUSDT. Treat this as replaceable configuration rather than a permanent popularity claim. Expose every interval supported by the adapter.

## D007 — Candle and signal semantics

Signals evaluate fixed-length contiguous windows of closed candles. `C1` is oldest and `CN` newest. Official markers anchor to the newest candle; optional span highlighting covers the complete window. Open-candle revisions may invalidate provisional evaluation, but do not create official markers.

## D008 — Chain semantics

Chains are ordered and non-overlapping. The gap is the number of complete candles strictly between the previous signal’s ending candle and the next signal’s starting candle. Supported modes are `exactly K` and `up to K` (`0..K`). Results are deterministic.

## D009 — Safe DSL execution

Signal text is parsed into a versioned AST and evaluated by an explicit interpreter. `eval` and equivalent dynamic execution are prohibited.

## D010 — Chart library constraint

Select a maintained chart library with a permissive license suitable for redistribution, after verifying candlestick, volume, crosshair, pan/zoom, custom markers, and optional span rendering requirements.

## D011 — Decision logging practice

Keep stable product intent in `DESCRIPTION.md`, active execution work in `PLAN.md`, and durable decisions/rationale here. Use commit messages and test documentation for implementation history rather than appending every development note to the product description.

## D012 — Canonical market-data contract

Provider adapters and the frontend exchange provider-neutral DTOs for instruments, intervals, candle history, stream selection, and candle upserts. Canonical candles contain provider, symbol, interval, UTC opening timestamp, OHLCV values, and closed state. The interval is part of candle identity so simultaneous timeframes cannot collide.

Timestamps are integer Unix epoch milliseconds and all display localization happens in the frontend. OHLCV values use finite IEEE-754 numbers because the first milestone is charting and deterministic signal evaluation rather than accounting or order execution. Provider adapters must validate identifiers, timestamps, price ranges, and numeric values while normalizing provider payloads.

Intervals use an opaque identifier plus an amount/unit definition. Month intervals remain calendar units rather than being approximated as a fixed number of milliseconds.

## D013 — Deterministic offline provider

The backend includes a provider-neutral mock/replay adapter with fixed instruments, intervals, timestamps, and generated OHLCV values. It performs no network access. Historical queries are timestamp ordered, use inclusive boundaries, and retain the newest matching candles when limited.

Replay subscriptions preserve fixture order rather than sanitizing it. This allows custom fixtures to exercise duplicates, revisions, and out-of-order delivery in the candle store while the built-in development fixture remains stable and valid. Subscription cancellation is explicit and also occurs when its handle is dropped.

## D014 — In-memory candle-store semantics

The provider-neutral candle store keys each series by provider, symbol, and interval, then keys candles by UTC opening timestamp. Inserts may arrive in any order. An exact repeat is a no-op, while a different candle with the same identity is a revision and the most recently received value wins. Ordered snapshots are isolated copies so callers cannot mutate cached state.

Gap detection compares adjacent stored timestamps against the selected interval definition. Seconds through weeks use checked fixed-duration arithmetic; months advance by UTC calendar months and are never approximated as a fixed number of days. Historical batches are validated in full before any candle is stored, preventing partially applied invalid responses.

## D015 — Binance public-history transport

Use Binance's unauthenticated market-data-only HTTPS host for Spot kline history. The adapter sends UTC millisecond bounds directly to `GET /api/v3/klines`, enforces the provider's 1,000-candle maximum, and converts the provider tuple response to canonical candles before returning it. A kline is closed only after its provider-supplied closing timestamp has passed.

Historical provider calls are asynchronous so external HTTPS requests never block a Tokio backend worker. Transport failures, malformed provider payloads, unknown streams, request-limit violations, and rate limiting remain distinct provider errors that the local API can translate without exposing Binance response types to callers.

## D016 — Binance live kline transport

Use Binance's unauthenticated market-data-only WebSocket host and one raw `<symbol>@kline_<interval>` stream per provider subscription. Stream names use Binance's required lowercase symbol while incoming events must match the requested canonical symbol and interval before their OHLCV strings and provider-supplied closed flag are converted into a provider-neutral candle upsert.

Subscriptions respond to WebSocket ping frames and close the connection when their cancellation handle is invoked or dropped. A connection or protocol failure ends the subscription; reconnect and overlap resynchronization are a separate Phase 2 task so their behavior can be implemented and tested together.

## D017 — Binance candle normalization boundary

REST history rows and WebSocket kline events pass through one private Binance normalization function before becoming canonical candles. The shared boundary assigns canonical stream identity, parses OHLCV decimal strings, validates opening and closing timestamps and price ranges, and preserves the transport-specific closed state.

All Binance payload structures remain private to the adapter. Downstream provider interfaces, storage, and rendering receive only provider-neutral candle history or candle-upsert events.
