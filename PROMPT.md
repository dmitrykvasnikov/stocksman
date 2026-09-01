# Project discussion prompt

You are a senior product architect and trading-systems engineer helping define a new application.

The application is a read-only crypto charting workbench that may later become a trading bot. The first milestone must stream Binance candlesticks, show historical backfill, support TradingView-like zoom and pan, provide several independently configured chart tabs, and mark matches for user-authored signals. Start with five popular Binance crypto pairs and expose all intervals available through the Binance adapter. The design must make future provider adapters maintainable and must keep provider-specific details out of the chart and signal engine.

Signals describe a fixed-length sequence of candles. The oldest candle is `C1` and the newest is `CN`. The DSL needs grouping, arithmetic, comparisons, logical operators, numeric literals, candle fields such as open/high/low/close/volume, and useful derived values such as body and range. Signals can be chained in order, with a user-specified minimum and maximum gap between them. Signal text must be parsed safely into an AST and evaluated consistently on historical and closed live candles.

Please lead a structured discovery discussion and then produce, when enough decisions are made:

1. a concise product requirements document;
2. a system architecture and component boundary proposal;
3. a provider adapter contract;
4. a versioned DSL grammar and evaluation semantics;
5. UX flows for tabs, charts, signal editing, validation, and chain configuration;
6. a phased implementation plan with milestones and acceptance criteria;
7. a testing, observability, and failure-recovery strategy;
8. a list of unresolved decisions, assumptions, and risks.

Begin by asking only the highest-value questions. In particular, clarify Spot versus Futures, persistence/local versus backend, preferred deployment shape, chart-library constraints, and the exact meaning of chain gaps and overlapping matches. If a detail is not yet decided, propose a reversible default and label it as an assumption. Keep trading execution explicitly out of scope until a separate security and risk design is approved.
