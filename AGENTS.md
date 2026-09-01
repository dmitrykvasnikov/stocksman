# AGENTS.md

## Project mission

Build a browser-based crypto market charting application that can later evolve into a trading bot. The first milestone is read-only live candlestick charts with custom signal overlays; order execution, account access, and automated trading are out of scope until explicitly designed and approved.

## Product boundaries

- Use Binance market data for the first adapter.
- Start with the five most popular Binance crypto pairs, selected by a documented rule (default proposal: BTCUSDT, ETHUSDT, BNBUSDT, SOLUSDT, and XRPUSDT).
- Expose every interval supported by Binance through the adapter rather than hard-coding a partial list.
- Provide multiple chart tabs, with an independently selected asset, timeframe, and signal set.
- Support pan, scroll, zoom, crosshair/tooltips, live updates, and historical backfill.
- Keep the UI read-only. Never add trading, API-key, withdrawal, or portfolio behavior to the initial milestone.

## Architecture rules

1. Keep market-data providers behind a provider-neutral interface. UI and signal evaluation must not import Binance-specific types.
2. Normalize provider data into canonical candles: `provider`, `symbol`, `timestamp`, `open`, `high`, `low`, `close`, `volume`, and `closed`.
3. Separate transport, normalization, storage/cache, signal parsing/evaluation, and chart rendering.
4. Make reconnects, rate limits, missing candles, duplicate events, and out-of-order events explicit behavior.
5. Signals must be deterministic and evaluable against both historical candles and the live stream.
6. Treat parsed DSL expressions as an AST. Do not evaluate user-entered text with `eval` or equivalent dynamic execution.
7. Validate signal length, references, operators, numeric values, and chained-signal gaps before saving.
8. Store timestamps in UTC and render them in the user’s selected locale/time zone.
9. Add tests for parsing, evaluation, candle-window boundaries, chaining, gap semantics, provider normalization, and reconnect behavior.
10. Keep a clear seam for adding future providers without changing the signal engine or chart UI.

## Suggested provider contract

```ts
interface MarketDataProvider {
  id: string;
  listSymbols(): Promise<Instrument[]>;
  listIntervals(): Promise<IntervalDefinition[]>;
  getCandles(request: CandleRequest): Promise<Candle[]>;
  subscribeCandles(request: CandleSubscription, onEvent: (event: CandleEvent) => void): Unsubscribe;
}
```

An adapter owns authentication requirements, symbols, interval mapping, REST backfill, WebSocket streaming, retries, and provider-specific error translation.

## Delivery practices

- Prefer small vertical slices that can be demonstrated.
- Document assumptions and unresolved product decisions in the repository.
- Include a local/mock provider so the chart and signal editor can be developed without a live network connection.
- Do not claim a feature is complete without a reproducible verification step.
- Keep secrets out of source control, logs, fixtures, and screenshots.
