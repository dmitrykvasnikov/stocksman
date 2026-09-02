/** Milliseconds since the Unix epoch in UTC. */
export type UtcTimestamp = number;

export interface Instrument {
  provider: string;
  symbol: string;
  base_asset: string;
  quote_asset: string;
}

export type IntervalUnit = "second" | "minute" | "hour" | "day" | "week" | "month";

export interface IntervalDefinition {
  id: string;
  label: string;
  amount: number;
  unit: IntervalUnit;
}

export interface Candle {
  provider: string;
  symbol: string;
  interval: string;
  timestamp: UtcTimestamp;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
  closed: boolean;
}

export interface CandleStream {
  provider: string;
  symbol: string;
  interval: string;
}

export interface CandleHistoryRequest extends CandleStream {
  /** Inclusive opening-time boundary. */
  start_timestamp: UtcTimestamp | null;
  /** Inclusive opening-time boundary. */
  end_timestamp: UtcTimestamp | null;
  limit: number | null;
}

export interface CandleHistoryResponse {
  candles: Candle[];
}

export type CandleEvent = {
  kind: "upsert";
  candle: Candle;
};
