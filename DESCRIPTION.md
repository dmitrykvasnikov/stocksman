# Live Crypto Charting and Signal Workbench

## One-sentence concept

A read-only, TradingView-like crypto charting workbench that streams Binance candlesticks, lets users switch between asset/timeframe tabs, and marks matches for custom signals written in a small safe DSL.

## Initial user experience

The application opens with five predefined popular Binance pairs. Each chart tab has its own symbol, interval, visible signals, and viewport. A user can:

- inspect historical and live-updating candlesticks;
- zoom and pan/scroll through history;
- hover candles for OHLCV details;
- select any interval reported as available by Binance;
- create, edit, enable, disable, and delete named signals;
- see a signal marker anchored to the last candle of every matching pattern;
- combine two or more signals into an ordered chain with a user-defined gap range.

## Proposed first release

### Market data

- Binance Spot public market data, initially.
- REST backfill plus WebSocket candle updates.
- Canonical internal candle model, independent of provider naming.
- Reconnect and resynchronization after disconnects.
- A mock/replay provider for tests and offline development.

### Charts

- Candlestick series with volume.
- Live updates without resetting the viewport.
- Pan, wheel/pinch zoom, reset view, crosshair, and OHLCV tooltip.
- Signal markers and optional pattern-span highlighting.
- Empty/loading/stale/error states that are visible to the user.

### Tabs and presets

Suggested initial pairs: `BTCUSDT`, `ETHUSDT`, `BNBUSDT`, `SOLUSDT`, `XRPUSDT`. Treat “most popular” as a replaceable configuration, not a permanent fact. Tabs should be closable and restorable, with a sensible default tab set.

Intervals should be populated from the Binance adapter’s supported-interval metadata. If Binance does not expose this as a runtime endpoint, keep the mapping in the adapter and test it against the provider documentation.

### Platform verification

Intermediate project phases are developed and verified on the primary development platform. Windows portability should be preserved throughout implementation, but a successful Windows build is not an acceptance requirement for those phases. Confirm the Windows build and packaging only in the final hardening and release phase, after all earlier project phases are complete.

## Signal DSL proposal

The DSL describes one fixed-length candle window. `C1` is the oldest candle and `CN` is the newest. The parser should know `N` from the declared pattern length.

### Candle fields

Use explicit names to avoid ambiguity:

```text
C1.open   C1.high   C1.low   C1.close   C1.volume
C1.body   C1.range  C1.upper_wick  C1.lower_wick
```

Derived fields are calculated by the engine. `min()` and `max()` should be ordinary functions over numeric arguments, while `lowest()` and `highest()` can be added later for ranges of candles.

### Operators and functions for the MVP

- Arithmetic: `+ - * / %`
- Comparison: `> >= < <= == !=`
- Logic: `and or not`
- Grouping: parentheses
- Numeric literals and decimals
- Functions: `abs(x)`, `min(a,b,...)`, `max(a,b,...)`
- Optional convenience: `bullish(C3)`, `bearish(C3)`, `body(C3)`, `range(C3)`

### Example signals

```text
pattern MorningStar length 3:
  bearish(C1) and C2.close > C1.close and bullish(C3)
  and C3.close > (C1.open + C1.close) / 2
```

```text
pattern CloseAbovePreviousHigh length 2:
  C2.close > C1.high and C2.volume >= C1.volume
```

The editor may initially use a length field plus a multiline expression, with syntax highlighting, validation messages, and a small field/operator help panel. A visual builder can be considered later.

## Signal matching semantics

For a signal of length `N`, evaluate every contiguous window of `N` closed candles. A match is anchored at the newest candle in that window. Do not mark an unfinished live candle unless the product explicitly adds an “intrabar” mode.

### Chained signals

A chain is an ordered list of signal names plus a gap constraint. If signal A ends at candle index `a` and signal B starts at `b`, define the gap clearly. Recommended MVP semantics:

```text
gap = number of complete candles strictly between A's ending candle and B's starting candle
minGap <= gap <= maxGap
```

Example: `A -> B gap 0..3` means B may begin immediately after A or after up to three intervening closed candles. The UI should show the matched spans and chain marker distinctly.

## Provider-neutral model

The signal engine should consume only canonical candles. A provider adapter should implement:

```text
list instruments
list supported intervals
fetch historical candles
subscribe to candle events
normalize provider events
```

Future adapters might include another exchange or an institutional market-data feed. Adding one should not require changes to DSL syntax, signal evaluation, or chart components.

## Non-goals for the initial milestone

- live or simulated order placement;
- exchange account authentication;
- portfolio, balances, P&L, or tax reporting;
- strategy optimization or backtesting UI;
- alerts, notifications, or cloud persistence;
- arbitrary third-party provider configuration by end users.

## Key decisions to discuss

1. Which frontend/charting library should be used, and what license constraints apply?
2. Should signals be stored locally first, or behind a small backend from day one?
3. Should the first version support Spot only, or also Binance Futures?
4. Should a signal marker represent the newest candle only, the full pattern span, or both?
5. Should chained signals allow overlapping matches?
6. What should happen when a candle is revised before it closes?
7. Is the first release desktop-first, responsive web, or an installable desktop app?
