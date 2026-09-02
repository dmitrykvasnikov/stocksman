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

export interface MarketDataCatalog {
  instruments: Instrument[];
  intervals: IntervalDefinition[];
}

export type CandleEvent = {
  kind: "upsert";
  candle: Candle;
};

async function readJson<T>(url: URL, signal?: AbortSignal): Promise<T> {
  const response = await fetch(url, { signal });
  if (!response.ok) {
    throw new Error(`Market-data request failed with status ${response.status}`);
  }
  return response.json() as Promise<T>;
}

export function loadMarketDataCatalog(
  endpoint: string,
  provider: string,
  signal?: AbortSignal,
): Promise<MarketDataCatalog> {
  const url = new URL("/market-data/catalog", endpoint);
  url.searchParams.set("provider", provider);
  return readJson(url, signal);
}

export async function loadCandleHistory(
  endpoint: string,
  request: CandleHistoryRequest,
  signal?: AbortSignal,
): Promise<CandleHistoryResponse> {
  const url = new URL("/market-data/candles", endpoint);
  url.searchParams.set("provider", request.provider);
  url.searchParams.set("symbol", request.symbol);
  url.searchParams.set("interval", request.interval);
  if (request.start_timestamp !== null) {
    url.searchParams.set("start_timestamp", String(request.start_timestamp));
  }
  if (request.end_timestamp !== null) {
    url.searchParams.set("end_timestamp", String(request.end_timestamp));
  }
  if (request.limit !== null) {
    url.searchParams.set("limit", String(request.limit));
  }

  return readJson(url, signal);
}
