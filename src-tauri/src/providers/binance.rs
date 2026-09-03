use std::{sync::Arc, time::Duration};

use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::Deserialize;

use crate::{
    market_data::{
        Candle, CandleHistoryRequest, CandleHistoryResponse, CandleStream, Instrument,
        IntervalDefinition, IntervalUnit,
    },
    provider::{
        CandleEventHandler, MarketDataProvider, ProviderError, ProviderFuture, ProviderResult,
        Unsubscribe,
    },
};

pub const BINANCE_PROVIDER_ID: &str = "binance";
pub const BINANCE_MARKET_DATA_BASE_URL: &str = "https://data-api.binance.vision";
const BINANCE_KLINE_LIMIT: u32 = 1_000;
const RESPONSE_MESSAGE_LIMIT: usize = 240;

type BinanceKline = (
    i64,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    u64,
    String,
    String,
    String,
);

#[derive(Clone)]
pub struct BinanceSpotProvider {
    client: Client,
    base_url: String,
    now_millis: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl BinanceSpotProvider {
    pub fn new() -> ProviderResult<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(12))
            .user_agent(concat!("stocksman/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;

        Ok(Self {
            client,
            base_url: BINANCE_MARKET_DATA_BASE_URL.to_owned(),
            now_millis: Arc::new(current_utc_millis),
        })
    }

    #[cfg(test)]
    fn with_base_url_and_clock(base_url: String, now_millis: i64) -> ProviderResult<Self> {
        let mut provider = Self::new()?;
        provider.base_url = base_url;
        provider.now_millis = Arc::new(move || now_millis);
        Ok(provider)
    }

    fn validate_request(&self, request: &CandleHistoryRequest) -> ProviderResult<()> {
        request.validate()?;
        if request.provider != BINANCE_PROVIDER_ID {
            return Err(ProviderError::UnknownProvider(request.provider.clone()));
        }
        if !default_instruments()
            .iter()
            .any(|instrument| instrument.symbol == request.symbol)
        {
            return Err(ProviderError::UnknownSymbol(request.symbol.clone()));
        }
        if !supported_intervals()
            .iter()
            .any(|interval| interval.id == request.interval)
        {
            return Err(ProviderError::UnknownInterval(request.interval.clone()));
        }
        if let Some(limit) = request.limit {
            if limit > BINANCE_KLINE_LIMIT {
                return Err(ProviderError::RequestLimitExceeded {
                    requested: limit,
                    maximum: BINANCE_KLINE_LIMIT,
                });
            }
        }
        Ok(())
    }

    async fn fetch_candles(
        &self,
        request: &CandleHistoryRequest,
    ) -> ProviderResult<CandleHistoryResponse> {
        self.validate_request(request)?;

        let mut query = vec![
            ("symbol", request.symbol.clone()),
            ("interval", request.interval.clone()),
        ];
        if let Some(start_timestamp) = request.start_timestamp {
            query.push(("startTime", start_timestamp.to_string()));
        }
        if let Some(end_timestamp) = request.end_timestamp {
            query.push(("endTime", end_timestamp.to_string()));
        }
        if let Some(limit) = request.limit {
            query.push(("limit", limit.to_string()));
        }

        let response = self
            .client
            .get(format!(
                "{}/api/v3/klines",
                self.base_url.trim_end_matches('/')
            ))
            .query(&query)
            .send()
            .await
            .map_err(|error| ProviderError::Transport(error.to_string()))?;

        if response.status() == StatusCode::TOO_MANY_REQUESTS
            || response.status() == StatusCode::IM_A_TEAPOT
        {
            let retry_after_seconds = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            return Err(ProviderError::RateLimited {
                retry_after_seconds,
            });
        }
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }

        let rows = response
            .json::<Vec<BinanceKline>>()
            .await
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let now_millis = (self.now_millis)();
        let mut candles = rows
            .into_iter()
            .map(|row| normalize_kline(row, request, now_millis))
            .collect::<ProviderResult<Vec<_>>>()?;
        candles.sort_by_key(|candle| candle.timestamp);

        Ok(CandleHistoryResponse { candles })
    }
}

impl MarketDataProvider for BinanceSpotProvider {
    fn id(&self) -> &str {
        BINANCE_PROVIDER_ID
    }

    fn list_symbols(&self) -> ProviderResult<Vec<Instrument>> {
        Ok(default_instruments())
    }

    fn list_intervals(&self) -> ProviderResult<Vec<IntervalDefinition>> {
        Ok(supported_intervals())
    }

    fn get_candles<'a>(
        &'a self,
        request: &'a CandleHistoryRequest,
    ) -> ProviderFuture<'a, CandleHistoryResponse> {
        Box::pin(self.fetch_candles(request))
    }

    fn subscribe_candles(
        &self,
        _request: &CandleStream,
        _on_event: CandleEventHandler,
    ) -> ProviderResult<Unsubscribe> {
        Err(ProviderError::UnsupportedOperation(
            "Binance WebSocket subscriptions",
        ))
    }
}

fn normalize_kline(
    row: BinanceKline,
    request: &CandleHistoryRequest,
    now_millis: i64,
) -> ProviderResult<Candle> {
    let (timestamp, open, high, low, close, volume, close_time, _, _, _, _, _) = row;
    let parse_number = |field: &'static str, value: String| {
        value.parse::<f64>().map_err(|_| {
            ProviderError::InvalidResponse(format!("{field} is not a finite decimal number"))
        })
    };
    let candle = Candle {
        provider: BINANCE_PROVIDER_ID.to_owned(),
        symbol: request.symbol.clone(),
        interval: request.interval.clone(),
        timestamp,
        open: parse_number("open", open)?,
        high: parse_number("high", high)?,
        low: parse_number("low", low)?,
        close: parse_number("close", close)?,
        volume: parse_number("volume", volume)?,
        closed: close_time < now_millis,
    };
    candle
        .validate()
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
    Ok(candle)
}

#[derive(Deserialize)]
struct BinanceApiError {
    code: i64,
    msg: String,
}

async fn response_error(response: reqwest::Response) -> ProviderError {
    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => return ProviderError::Transport(error.to_string()),
    };
    if let Ok(error) = serde_json::from_str::<BinanceApiError>(&body) {
        return match error.code {
            -1120 => ProviderError::UnknownInterval(error.msg),
            -1121 => ProviderError::UnknownSymbol(error.msg),
            _ => ProviderError::InvalidResponse(format!(
                "Binance returned HTTP {status} ({}): {}",
                error.code,
                truncate_message(&error.msg)
            )),
        };
    }

    ProviderError::InvalidResponse(format!(
        "Binance returned HTTP {status}: {}",
        truncate_message(&body)
    ))
}

fn truncate_message(message: &str) -> String {
    message.chars().take(RESPONSE_MESSAGE_LIMIT).collect()
}

fn current_utc_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

fn default_instruments() -> Vec<Instrument> {
    ["BTC", "ETH", "BNB", "SOL", "XRP"]
        .into_iter()
        .map(|base_asset| Instrument {
            provider: BINANCE_PROVIDER_ID.to_owned(),
            symbol: format!("{base_asset}USDT"),
            base_asset: base_asset.to_owned(),
            quote_asset: "USDT".to_owned(),
        })
        .collect()
}

fn supported_intervals() -> Vec<IntervalDefinition> {
    use IntervalUnit::{Day, Hour, Minute, Month, Second, Week};

    [
        ("1s", "1 second", 1, Second),
        ("1m", "1 minute", 1, Minute),
        ("3m", "3 minutes", 3, Minute),
        ("5m", "5 minutes", 5, Minute),
        ("15m", "15 minutes", 15, Minute),
        ("30m", "30 minutes", 30, Minute),
        ("1h", "1 hour", 1, Hour),
        ("2h", "2 hours", 2, Hour),
        ("4h", "4 hours", 4, Hour),
        ("6h", "6 hours", 6, Hour),
        ("8h", "8 hours", 8, Hour),
        ("12h", "12 hours", 12, Hour),
        ("1d", "1 day", 1, Day),
        ("3d", "3 days", 3, Day),
        ("1w", "1 week", 1, Week),
        ("1M", "1 month", 1, Month),
    ]
    .into_iter()
    .map(|(id, label, amount, unit)| IntervalDefinition {
        id: id.to_owned(),
        label: label.to_owned(),
        amount,
        unit,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;

    fn history_request() -> CandleHistoryRequest {
        CandleHistoryRequest {
            provider: BINANCE_PROVIDER_ID.to_owned(),
            symbol: "BTCUSDT".to_owned(),
            interval: "1m".to_owned(),
            start_timestamp: Some(1_704_067_200_000),
            end_timestamp: Some(1_704_067_320_000),
            limit: Some(3),
        }
    }

    #[tokio::test]
    async fn public_history_request_is_normalized_to_canonical_candles() {
        let body = r#"[
            [1704067260000,"101.5","104.0","100.0","103.0","12.25",1704067319999,"0",8,"0","0","0"],
            [1704067200000,"100.0","102.0","99.0","101.5","10.5",1704067259999,"0",5,"0","0","0"],
            [1704067320000,"103.0","105.0","102.5","104.5","7.0",1704067379999,"0",3,"0","0","0"]
        ]"#;
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("address"));
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("connection");
            let mut request = vec![0; 4_096];
            let length = stream.read(&mut request).await.expect("request");
            let request = String::from_utf8(request[..length].to_vec()).expect("UTF-8 request");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("response");
            request
        });
        let provider = BinanceSpotProvider::with_base_url_and_clock(base_url, 1_704_067_350_000)
            .expect("provider");

        let history = provider
            .get_candles(&history_request())
            .await
            .expect("history");
        let raw_request = server.await.expect("server task");

        assert!(raw_request.starts_with("GET /api/v3/klines?"));
        assert!(raw_request.contains("symbol=BTCUSDT"));
        assert!(raw_request.contains("interval=1m"));
        assert!(raw_request.contains("startTime=1704067200000"));
        assert!(raw_request.contains("endTime=1704067320000"));
        assert!(raw_request.contains("limit=3"));
        assert_eq!(history.candles.len(), 3);
        assert_eq!(history.candles[0].timestamp, 1_704_067_200_000);
        assert_eq!(history.candles[0].open, 100.0);
        assert!(history.candles[0].closed);
        assert_eq!(history.candles[2].close, 104.5);
        assert!(!history.candles[2].closed);
        assert!(history.candles.iter().all(|candle| {
            candle.provider == BINANCE_PROVIDER_ID
                && candle.symbol == "BTCUSDT"
                && candle.interval == "1m"
        }));
    }

    #[tokio::test]
    async fn provider_limit_is_checked_before_network_access() {
        let provider =
            BinanceSpotProvider::with_base_url_and_clock("http://127.0.0.1:1".to_owned(), i64::MAX)
                .expect("provider");
        let mut request = history_request();
        request.limit = Some(BINANCE_KLINE_LIMIT + 1);

        assert_eq!(
            provider.get_candles(&request).await,
            Err(ProviderError::RequestLimitExceeded {
                requested: 1_001,
                maximum: 1_000,
            })
        );
    }

    #[test]
    fn adapter_exposes_configured_symbols_and_all_spot_kline_intervals() {
        let provider = BinanceSpotProvider::new().expect("provider");

        assert_eq!(provider.list_symbols().expect("symbols").len(), 5);
        assert_eq!(provider.list_intervals().expect("intervals").len(), 16);
        assert_eq!(
            provider
                .list_intervals()
                .expect("intervals")
                .last()
                .expect("monthly interval")
                .id,
            "1M"
        );
    }
}
