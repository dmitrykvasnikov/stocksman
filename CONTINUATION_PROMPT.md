# Continuation prompt for project discovery

You are continuing product and system design for the project in this repository. First read `AGENTS.md`, `DESCRIPTION.md`, `PROMPT.md`, and this file. Treat the repository instructions as authoritative.

Do not implement trading execution. The first milestone is strictly a read-only Binance Spot charting workbench with live candlestick data, historical backfill, multiple chart tabs, and safe custom signal visualization.

## Decisions already made

- Market: Binance Spot only.
- Product shape: installable desktop application.
- Backend: present from the beginning; it owns Binance REST/WebSocket connectivity and serves normalized market data to the desktop UI.
- Deployment: backend should run locally first while retaining a clean path to hosted deployment later.
- User model: single user initially; no login or multi-device synchronization in the first release.
- Persistence: persist signals, chained-signal definitions, enabled states, and user preferences. Do not persist historical market data; historical candles may be cached in memory and fetched on demand.
- Proposed stack: Tauri 2, React, TypeScript, Rust/Tokio backend/core, and SQLite. Treat this as the current recommendation and validate it during discussion rather than silently changing it.
- Development environment: Arch Linux with i3wm and X11. The application must remain portable to Windows. Prefer platform-neutral APIs and CI builds for Linux and Windows.
- Chart library: still undecided; choose after the frontend/desktop stack and licensing constraints are reviewed.
- Initial pairs: BTCUSDT, ETHUSDT, BNBUSDT, SOLUSDT, and XRPUSDT, selected through replaceable configuration rather than a permanent popularity claim.
- Intervals: expose every interval supported by the Binance adapter.
- Open candles: revisions may update or invalidate provisional evaluation, but official signal markers are created only for closed candles.
- Markers: show the newest-candle anchor and support optional full-pattern-span highlighting.
- Chains: ordered, non-overlapping signal matches. Report all valid non-overlapping combinations deterministically.
- Chain gap meaning: the number of complete candles strictly between the previous signal's ending candle and the next signal's starting candle.
- Chain gap modes: `exactly K` or `up to K` (`0..K`). A zero gap means the next pattern begins immediately after the previous pattern ends.

## Architecture constraints

Keep provider-specific types inside the Binance adapter. The signal engine and UI must consume canonical candles only. Separate transport, normalization, in-memory storage/cache, parsing/evaluation, persistence, and chart rendering. Parse signal text into a versioned AST; never evaluate user text with `eval` or equivalent. Make reconnects, rate limits, missing candles, duplicate events, out-of-order events, and candle revisions explicit behavior.

## How to continue

Continue the structured discovery discussion requested by `PROMPT.md`. Ask only the highest-value unresolved questions, and propose reversible defaults when a decision is not essential yet. Do not ask again about decisions listed above.

Once the decisions are sufficiently complete, produce:

1. a concise product requirements document;
2. a component and system architecture proposal;
3. a provider adapter contract;
4. a versioned DSL grammar and evaluation semantics;
5. UX flows for tabs, charts, signal editing, validation, and chain configuration;
6. a phased implementation plan with milestones and acceptance criteria;
7. testing, observability, and failure-recovery strategy;
8. unresolved decisions, assumptions, risks, and explicit non-goals.

Keep order execution, account access, API keys, withdrawals, balances, portfolio behavior, alerts, notifications, and automated trading out of scope until a separate security and risk design is approved.
