# Implementation Plan

This plan turns the read-only Binance Spot charting workbench discovery into small, demonstrable vertical slices.

## [ ] Phase 0 — Foundation

- [x] Scaffold Tauri 2, React, TypeScript, Rust, and Tokio.
- [x] Start and supervise a private local backend sidecar over loopback.
- [x] Add SQLite migrations and persisted user configuration.
- [x] Add Linux and Windows CI build targets.
- [ ] Define provider-neutral API DTOs and canonical candle types.

Acceptance criteria:

- [x] The desktop shell starts the backend and reports ready, reconnecting, and unavailable states.
- [x] The backend is reachable only through loopback.
- [ ] A clean build is reproducible on Linux and has a Windows CI target.

## [ ] Phase 1 — Mock market-data vertical slice

- [ ] Implement a mock/replay provider.
- [ ] Implement the in-memory candle store, ordering, deduplication, and gap detection.
- [ ] Render a basic candlestick and volume chart.
- [ ] Support one configurable chart tab.

Acceptance criteria:

- [ ] A deterministic candle replay renders without Binance or network access.
- [ ] Duplicate and out-of-order events produce one correctly ordered candle series.
- [ ] The chart can pan, zoom, reset, and inspect OHLCV values.

## [ ] Phase 2 — Binance Spot adapter

- [ ] Implement public REST historical backfill.
- [ ] Implement WebSocket kline subscriptions.
- [ ] Normalize all provider events into canonical candles.
- [ ] Add reconnect, overlap resynchronization, rate-limit, missing-candle, and revision handling.
- [ ] Load the five configured default pairs and every interval supported by the adapter.

Acceptance criteria:

- [ ] Historical candles load and live closed-candle updates arrive for each default pair.
- [ ] Reconnects do not create duplicates or silently lose a candle range.
- [ ] Provider-specific types remain inside the Binance adapter.

## [ ] Phase 3 — Chart workbench

- [ ] Select and verify a permissively licensed chart library.
- [ ] Add multiple independent tabs.
- [ ] Add symbol, interval, signal visibility, and viewport controls.
- [ ] Persist tab and display preferences.
- [ ] Add loading, stale, gap, error, and reconnect states.

Acceptance criteria:

- [ ] Each tab has independent symbol, interval, signals, and viewport state.
- [ ] Live updates preserve the viewport unless the user is following the latest candle.
- [ ] Closing and reopening the app restores persisted workspace configuration.

## [ ] Phase 4 — Signal DSL and visualization

- [ ] Implement the versioned DSL v1 lexer and parser.
- [ ] Store source text, AST, and DSL version.
- [ ] Validate references, types, operators, numbers, functions, and expression limits.
- [ ] Evaluate contiguous windows of closed candles.
- [ ] Add newest-candle markers and optional full-pattern spans.
- [ ] Re-evaluate affected windows after candle revisions.

Acceptance criteria:

- [ ] Invalid signal text cannot be saved.
- [ ] Identical closed candle input produces identical historical and live results.
- [ ] Open candles never receive official signal markers.
- [ ] User-entered text is never dynamically executed.

## [ ] Phase 5 — Signal chains

- [ ] Add ordered chain definitions and persistence.
- [ ] Support `exactly K` and `up to K` (`0..K`) gaps.
- [ ] Enforce non-overlapping matches.
- [ ] Produce deterministic combinations with a safe result cap and truncation state.

Acceptance criteria:

- [ ] Gap and overlap behavior matches the documented semantics in `DECISIONS.md`.
- [ ] Results are stable across repeated evaluations.
- [ ] Excessive chain combinations remain bounded and visible to the user.

## [ ] Phase 6 — Hardening and release

- [ ] Add migrations, diagnostics, crash recovery, and backend restart handling.
- [ ] Complete Linux and Windows packaging smoke tests.
- [ ] Verify no accounts, credentials, trading, portfolio, alert, or notification behavior has entered the product.
- [ ] Document reproducible verification commands for each milestone.

Acceptance criteria:

- [ ] The app recovers from backend restart and network interruption.
- [ ] Automated tests cover parsing, evaluation, boundaries, chaining, normalization, gaps, revisions, and reconnects.
- [ ] The first release remains strictly read-only.

## Working method

Implement one phase at a time. Use `[ ]` for pending, `[~]` for in progress, and `[x]` for completed work. Keep each change small enough to demonstrate and verify. Update this file when scope, ordering, or acceptance criteria change. Record durable decisions and their rationale in `DECISIONS.md`; do not turn `DESCRIPTION.md` into a running development diary.
