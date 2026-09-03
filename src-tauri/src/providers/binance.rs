use std::{collections::BTreeMap, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::Deserialize;
use tokio::{runtime::Handle, sync::oneshot, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    market_data::{
        Candle, CandleEvent, CandleHistoryRequest, CandleHistoryResponse, CandleStream, Instrument,
        IntervalDefinition, IntervalUnit,
    },
    provider::{
        CandleEventHandler, MarketDataProvider, ProviderError, ProviderFuture, ProviderResult,
        Unsubscribe,
    },
};

pub const BINANCE_PROVIDER_ID: &str = "binance";
pub const BINANCE_MARKET_DATA_BASE_URL: &str = "https://data-api.binance.vision";
pub const BINANCE_MARKET_DATA_WS_BASE_URL: &str = "wss://data-stream.binance.vision:443/ws";
const BINANCE_KLINE_LIMIT: u32 = 1_000;
const RESPONSE_MESSAGE_LIMIT: usize = 240;
const RECONNECT_INITIAL_DELAY: Duration = Duration::from_millis(500);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);
const RECENT_CANDLE_LIMIT: usize = 2_048;

type BinanceRestKline = (
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

struct BinanceKlineValues {
    timestamp: i64,
    close_time: i64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

#[derive(Clone)]
pub struct BinanceSpotProvider {
    client: Client,
    base_url: String,
    websocket_base_url: String,
    now_millis: Arc<dyn Fn() -> i64 + Send + Sync>,
    reconnect_initial_delay: Duration,
    reconnect_max_delay: Duration,
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
            websocket_base_url: BINANCE_MARKET_DATA_WS_BASE_URL.to_owned(),
            now_millis: Arc::new(current_utc_millis),
            reconnect_initial_delay: RECONNECT_INITIAL_DELAY,
            reconnect_max_delay: RECONNECT_MAX_DELAY,
        })
    }

    #[cfg(test)]
    fn with_base_url_and_clock(base_url: String, now_millis: i64) -> ProviderResult<Self> {
        let mut provider = Self::new()?;
        provider.base_url = base_url;
        provider.now_millis = Arc::new(move || now_millis);
        Ok(provider)
    }

    #[cfg(test)]
    fn with_websocket_base_url(websocket_base_url: String) -> ProviderResult<Self> {
        let mut provider = Self::new()?;
        provider.websocket_base_url = websocket_base_url;
        Ok(provider)
    }

    #[cfg(test)]
    fn with_test_endpoints(base_url: String, websocket_base_url: String) -> ProviderResult<Self> {
        let mut provider = Self::new()?;
        provider.base_url = base_url;
        provider.websocket_base_url = websocket_base_url;
        provider.reconnect_initial_delay = Duration::from_millis(10);
        provider.reconnect_max_delay = Duration::from_millis(20);
        Ok(provider)
    }

    fn validate_stream(&self, stream: &CandleStream) -> ProviderResult<()> {
        stream.validate()?;
        if stream.provider != BINANCE_PROVIDER_ID {
            return Err(ProviderError::UnknownProvider(stream.provider.clone()));
        }
        if !default_instruments()
            .iter()
            .any(|instrument| instrument.symbol == stream.symbol)
        {
            return Err(ProviderError::UnknownSymbol(stream.symbol.clone()));
        }
        if !supported_intervals()
            .iter()
            .any(|interval| interval.id == stream.interval)
        {
            return Err(ProviderError::UnknownInterval(stream.interval.clone()));
        }
        Ok(())
    }

    fn validate_request(&self, request: &CandleHistoryRequest) -> ProviderResult<()> {
        request.validate()?;
        self.validate_stream(&CandleStream {
            provider: request.provider.clone(),
            symbol: request.symbol.clone(),
            interval: request.interval.clone(),
        })?;
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
            .json::<Vec<BinanceRestKline>>()
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
        request: &CandleStream,
        on_event: CandleEventHandler,
    ) -> ProviderResult<Unsubscribe> {
        self.validate_stream(request)?;
        let runtime = Handle::try_current().map_err(|_| ProviderError::RuntimeUnavailable)?;
        let stream = request.clone();
        let url = format!(
            "{}/{}@kline_{}",
            self.websocket_base_url.trim_end_matches('/'),
            request.symbol.to_ascii_lowercase(),
            request.interval
        );
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();

        runtime.spawn(
            self.clone()
                .run_subscription(url, stream, on_event, shutdown_receiver),
        );

        Ok(Unsubscribe::new(shutdown_sender))
    }
}

impl BinanceSpotProvider {
    async fn run_subscription(
        self,
        url: String,
        stream: CandleStream,
        on_event: CandleEventHandler,
        mut shutdown: oneshot::Receiver<()>,
    ) {
        let mut cursor = SubscriptionCursor::default();
        let mut retry_attempt = 0_u32;

        loop {
            let connection = tokio::select! {
                _ = &mut shutdown => return,
                connection = connect_async(&url) => connection,
            };
            let (mut socket, _) = match connection {
                Ok(connection) => connection,
                Err(error) => {
                    eprintln!("Binance WebSocket connection failed: {error}");
                    if wait_for_retry(
                        &mut shutdown,
                        reconnect_delay(
                            self.reconnect_initial_delay,
                            self.reconnect_max_delay,
                            retry_attempt,
                        ),
                    )
                    .await
                    {
                        return;
                    }
                    retry_attempt = retry_attempt.saturating_add(1);
                    continue;
                }
            };

            if let Some(overlap_start) = cursor.latest_timestamp() {
                match self
                    .recover_candles(
                        &stream,
                        overlap_start,
                        None,
                        &on_event,
                        &mut cursor,
                        &mut shutdown,
                    )
                    .await
                {
                    RecoveryOutcome::Complete => {}
                    RecoveryOutcome::Cancelled => {
                        let _ = socket.close(None).await;
                        return;
                    }
                    RecoveryOutcome::Failed(error) => {
                        eprintln!("Binance overlap resynchronization failed: {error}");
                        let _ = socket.close(None).await;
                        if wait_for_retry(
                            &mut shutdown,
                            reconnect_delay(
                                self.reconnect_initial_delay,
                                self.reconnect_max_delay,
                                retry_attempt,
                            ),
                        )
                        .await
                        {
                            return;
                        }
                        retry_attempt = retry_attempt.saturating_add(1);
                        continue;
                    }
                }
            }

            let reconnect = loop {
                tokio::select! {
                    _ = &mut shutdown => {
                        let _ = socket.close(None).await;
                        return;
                    }
                    message = socket.next() => match message {
                        Some(Ok(Message::Text(payload))) => {
                            let event = match normalize_stream_event(payload.as_ref(), &stream) {
                                Ok(event) => event,
                                Err(error) => {
                                    eprintln!("invalid Binance WebSocket event: {error}");
                                    break true;
                                }
                            };
                            let CandleEvent::Upsert { candle } = event;
                            if let Some(gap_start) = cursor.gap_start(candle.timestamp, &stream.interval) {
                                match self
                                    .recover_candles(
                                        &stream,
                                        gap_start,
                                        Some(candle.timestamp),
                                        &on_event,
                                        &mut cursor,
                                        &mut shutdown,
                                    )
                                    .await
                                {
                                    RecoveryOutcome::Complete => {}
                                    RecoveryOutcome::Cancelled => {
                                        let _ = socket.close(None).await;
                                        return;
                                    }
                                    RecoveryOutcome::Failed(error) => {
                                        eprintln!("Binance missing-candle recovery failed: {error}");
                                        break true;
                                    }
                                }
                            }
                            cursor.emit(candle, &on_event);
                            retry_attempt = 0;
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            if let Err(error) = socket.send(Message::Pong(payload)).await {
                                eprintln!("Binance WebSocket pong failed: {error}");
                                break true;
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break true,
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            eprintln!("Binance WebSocket stream failed: {error}");
                            break true;
                        }
                    }
                }
            };

            if reconnect {
                if wait_for_retry(
                    &mut shutdown,
                    reconnect_delay(
                        self.reconnect_initial_delay,
                        self.reconnect_max_delay,
                        retry_attempt,
                    ),
                )
                .await
                {
                    return;
                }
                retry_attempt = retry_attempt.saturating_add(1);
            }
        }
    }

    async fn recover_candles(
        &self,
        stream: &CandleStream,
        start_timestamp: i64,
        end_timestamp: Option<i64>,
        on_event: &CandleEventHandler,
        cursor: &mut SubscriptionCursor,
        shutdown: &mut oneshot::Receiver<()>,
    ) -> RecoveryOutcome {
        let mut page_start = start_timestamp;
        let mut rate_limit_attempt = 0_u32;

        loop {
            let request = CandleHistoryRequest {
                provider: stream.provider.clone(),
                symbol: stream.symbol.clone(),
                interval: stream.interval.clone(),
                start_timestamp: Some(page_start),
                end_timestamp,
                limit: Some(BINANCE_KLINE_LIMIT),
            };
            let response = tokio::select! {
                _ = &mut *shutdown => return RecoveryOutcome::Cancelled,
                response = self.fetch_candles(&request) => response,
            };
            let history = match response {
                Ok(history) => history,
                Err(ProviderError::RateLimited {
                    retry_after_seconds,
                }) => {
                    let delay = retry_after_seconds
                        .map(Duration::from_secs)
                        .unwrap_or_else(|| {
                            reconnect_delay(
                                self.reconnect_initial_delay,
                                self.reconnect_max_delay,
                                rate_limit_attempt,
                            )
                        });
                    eprintln!("Binance recovery rate limited; retrying after {delay:?}");
                    if wait_for_retry(shutdown, delay).await {
                        return RecoveryOutcome::Cancelled;
                    }
                    rate_limit_attempt = rate_limit_attempt.saturating_add(1);
                    continue;
                }
                Err(error) => return RecoveryOutcome::Failed(error),
            };
            rate_limit_attempt = 0;

            let candle_count = history.candles.len();
            let Some(last_timestamp) = history.candles.last().map(|candle| candle.timestamp) else {
                return RecoveryOutcome::Complete;
            };
            for candle in history.candles {
                cursor.emit(candle, on_event);
            }

            if candle_count < BINANCE_KLINE_LIMIT as usize
                || end_timestamp.is_some_and(|end| last_timestamp >= end)
            {
                return RecoveryOutcome::Complete;
            }
            let Some(next_page) = last_timestamp.checked_add(1) else {
                return RecoveryOutcome::Failed(ProviderError::InvalidResponse(
                    "recovery timestamp overflowed".to_owned(),
                ));
            };
            if next_page <= page_start {
                return RecoveryOutcome::Failed(ProviderError::InvalidResponse(
                    "recovery history did not advance".to_owned(),
                ));
            }
            page_start = next_page;
        }
    }
}

#[derive(Debug)]
enum RecoveryOutcome {
    Complete,
    Cancelled,
    Failed(ProviderError),
}

#[derive(Default)]
struct SubscriptionCursor {
    recent: BTreeMap<i64, Candle>,
}

impl SubscriptionCursor {
    fn latest_timestamp(&self) -> Option<i64> {
        self.recent
            .last_key_value()
            .map(|(timestamp, _)| *timestamp)
    }

    fn gap_start(&self, timestamp: i64, interval: &str) -> Option<i64> {
        let latest = self.latest_timestamp()?;
        let expected = advance_binance_interval(latest, interval)?;
        (timestamp > expected).then_some(latest)
    }

    fn emit(&mut self, candle: Candle, on_event: &CandleEventHandler) {
        if self.recent.get(&candle.timestamp) == Some(&candle) {
            return;
        }
        self.recent.insert(candle.timestamp, candle.clone());
        while self.recent.len() > RECENT_CANDLE_LIMIT {
            self.recent.pop_first();
        }
        on_event(CandleEvent::Upsert { candle });
    }
}

fn advance_binance_interval(timestamp: i64, interval: &str) -> Option<i64> {
    let definition = supported_intervals()
        .into_iter()
        .find(|definition| definition.id == interval)?;
    if definition.unit == IntervalUnit::Month {
        use chrono::{DateTime, Months, Utc};

        return DateTime::<Utc>::from_timestamp_millis(timestamp)?
            .checked_add_months(Months::new(definition.amount))
            .map(|date_time| date_time.timestamp_millis());
    }
    let unit_millis = match definition.unit {
        IntervalUnit::Second => 1_000_i64,
        IntervalUnit::Minute => 60_000,
        IntervalUnit::Hour => 3_600_000,
        IntervalUnit::Day => 86_400_000,
        IntervalUnit::Week => 604_800_000,
        IntervalUnit::Month => unreachable!(),
    };
    timestamp.checked_add(unit_millis.checked_mul(i64::from(definition.amount))?)
}

fn reconnect_delay(initial: Duration, maximum: Duration, attempt: u32) -> Duration {
    initial
        .checked_mul(2_u32.saturating_pow(attempt.min(16)))
        .unwrap_or(maximum)
        .min(maximum)
}

async fn wait_for_retry(shutdown: &mut oneshot::Receiver<()>, delay: Duration) -> bool {
    tokio::select! {
        _ = &mut *shutdown => true,
        _ = sleep(delay) => false,
    }
}

#[derive(Deserialize)]
struct BinanceKlineEvent {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "E")]
    event_timestamp: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "k")]
    kline: BinanceStreamKline,
}

#[derive(Deserialize)]
struct BinanceStreamKline {
    #[serde(rename = "t")]
    timestamp: i64,
    #[serde(rename = "T")]
    close_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    closed: bool,
}

fn normalize_stream_event(payload: &str, stream: &CandleStream) -> ProviderResult<CandleEvent> {
    let event = serde_json::from_str::<BinanceKlineEvent>(payload)
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
    if event.event_type != "kline" {
        return Err(ProviderError::InvalidResponse(format!(
            "unexpected event type {}",
            event.event_type
        )));
    }
    if event.symbol != stream.symbol || event.kline.symbol != stream.symbol {
        return Err(ProviderError::InvalidResponse(
            "kline symbol does not match its subscription".to_owned(),
        ));
    }
    if event.kline.interval != stream.interval {
        return Err(ProviderError::InvalidResponse(
            "kline interval does not match its subscription".to_owned(),
        ));
    }

    validate_provider_timestamp("event timestamp", event.event_timestamp)?;
    let candle = normalize_candle(
        BinanceKlineValues {
            timestamp: event.kline.timestamp,
            close_time: event.kline.close_time,
            open: event.kline.open,
            high: event.kline.high,
            low: event.kline.low,
            close: event.kline.close,
            volume: event.kline.volume,
        },
        stream,
        event.kline.closed,
    )?;
    Ok(CandleEvent::Upsert { candle })
}

fn normalize_kline(
    row: BinanceRestKline,
    request: &CandleHistoryRequest,
    now_millis: i64,
) -> ProviderResult<Candle> {
    let (timestamp, open, high, low, close, volume, close_time, _, _, _, _, _) = row;
    normalize_candle(
        BinanceKlineValues {
            timestamp,
            close_time,
            open,
            high,
            low,
            close,
            volume,
        },
        &CandleStream {
            provider: request.provider.clone(),
            symbol: request.symbol.clone(),
            interval: request.interval.clone(),
        },
        close_time < now_millis,
    )
}

fn normalize_candle(
    values: BinanceKlineValues,
    stream: &CandleStream,
    closed: bool,
) -> ProviderResult<Candle> {
    validate_provider_timestamp("opening timestamp", values.timestamp)?;
    validate_provider_timestamp("closing timestamp", values.close_time)?;
    if values.close_time < values.timestamp {
        return Err(ProviderError::InvalidResponse(
            "closing timestamp precedes opening timestamp".to_owned(),
        ));
    }

    let candle = Candle {
        provider: BINANCE_PROVIDER_ID.to_owned(),
        symbol: stream.symbol.clone(),
        interval: stream.interval.clone(),
        timestamp: values.timestamp,
        open: parse_decimal("open", values.open)?,
        high: parse_decimal("high", values.high)?,
        low: parse_decimal("low", values.low)?,
        close: parse_decimal("close", values.close)?,
        volume: parse_decimal("volume", values.volume)?,
        closed,
    };
    candle
        .validate()
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
    Ok(candle)
}

fn validate_provider_timestamp(field: &'static str, value: i64) -> ProviderResult<()> {
    if value < 0 {
        return Err(ProviderError::InvalidResponse(format!(
            "{field} must be a non-negative UTC millisecond timestamp"
        )));
    }
    Ok(())
}

fn parse_decimal(field: &'static str, value: String) -> ProviderResult<f64> {
    value.parse::<f64>().map_err(|_| {
        ProviderError::InvalidResponse(format!("{field} is not a finite decimal number"))
    })
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
    use std::sync::Arc;

    use crate::candle_store::CandleStore;
    use futures_util::{SinkExt, StreamExt};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::mpsc,
        time::timeout,
    };
    use tokio_tungstenite::{accept_hdr_async, tungstenite::Message};

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

    fn candle_stream() -> CandleStream {
        CandleStream {
            provider: BINANCE_PROVIDER_ID.to_owned(),
            symbol: "BTCUSDT".to_owned(),
            interval: "1m".to_owned(),
        }
    }

    fn websocket_payload(timestamp: i64, close_time: i64, close: &str) -> String {
        format!(
            r#"{{
                "e":"kline",
                "E":{close_time},
                "s":"BTCUSDT",
                "k":{{
                    "t":{timestamp},
                    "T":{close_time},
                    "s":"BTCUSDT",
                    "i":"1m",
                    "o":"100.0",
                    "c":"{close}",
                    "h":"104.0",
                    "l":"99.0",
                    "v":"10.5",
                    "x":true
                }}
            }}"#
        )
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

    #[tokio::test]
    #[allow(clippy::result_large_err)] // Signature is fixed by tungstenite's handshake callback.
    async fn websocket_kline_is_normalized_and_cancellation_closes_the_stream() {
        let payload = r#"{
            "e":"kline",
            "E":1704067259000,
            "s":"BTCUSDT",
            "k":{
                "t":1704067200000,
                "T":1704067259999,
                "s":"BTCUSDT",
                "i":"1m",
                "f":100,
                "L":200,
                "o":"100.0",
                "c":"101.5",
                "h":"102.0",
                "l":"99.0",
                "v":"10.5",
                "n":42,
                "x":true,
                "q":"0",
                "V":"0",
                "Q":"0",
                "B":"0"
            }
        }"#;
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let websocket_base_url = format!("ws://{}/ws", listener.local_addr().expect("address"));
        let server = tokio::spawn(async move {
            let (connection, _) = listener.accept().await.expect("connection");
            let mut socket = accept_hdr_async(
                connection,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.uri().path(), "/ws/btcusdt@kline_1m");
                    Ok(response)
                },
            )
            .await
            .expect("WebSocket handshake");

            socket
                .send(Message::Ping(vec![1, 2, 3]))
                .await
                .expect("send ping");
            assert_eq!(
                timeout(Duration::from_secs(1), socket.next())
                    .await
                    .expect("pong arrives")
                    .expect("stream remains open")
                    .expect("valid pong"),
                Message::Pong(vec![1, 2, 3])
            );
            socket
                .send(Message::Text(payload.into()))
                .await
                .expect("send kline");

            assert!(matches!(
                timeout(Duration::from_secs(1), socket.next())
                    .await
                    .expect("close arrives")
                    .expect("close frame is present")
                    .expect("valid close frame"),
                Message::Close(_)
            ));
        });
        let provider =
            BinanceSpotProvider::with_websocket_base_url(websocket_base_url).expect("provider");
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let subscription = provider
            .subscribe_candles(
                &candle_stream(),
                Arc::new(move |event| sender.send(event).expect("receiver remains open")),
            )
            .expect("subscription");

        let CandleEvent::Upsert { candle } = timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect("kline arrives")
            .expect("subscription remains open");
        assert_eq!(
            candle,
            Candle {
                provider: BINANCE_PROVIDER_ID.to_owned(),
                symbol: "BTCUSDT".to_owned(),
                interval: "1m".to_owned(),
                timestamp: 1_704_067_200_000,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.5,
                volume: 10.5,
                closed: true,
            }
        );

        drop(subscription);
        server.await.expect("server task");
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)] // Signature is fixed by tungstenite's handshake callback.
    async fn reconnect_recovers_overlap_gaps_rate_limits_and_revisions() {
        let first_timestamp = 1_704_067_200_000;
        let second_timestamp = first_timestamp + 60_000;
        let third_timestamp = second_timestamp + 60_000;

        let rest_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("REST listener");
        let base_url = format!(
            "http://{}",
            rest_listener.local_addr().expect("REST address")
        );
        let rest_server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response_number in 0..2 {
                let (mut stream, _) = rest_listener.accept().await.expect("REST connection");
                let mut request = vec![0; 4_096];
                let length = stream.read(&mut request).await.expect("REST request");
                requests.push(
                    String::from_utf8(request[..length].to_vec()).expect("UTF-8 REST request"),
                );

                let response = if response_number == 0 {
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
                } else {
                    let body = format!(
                        r#"[
                            [{first_timestamp},"100.0","104.0","99.0","101.5","10.5",{},"0",5,"0","0","0"],
                            [{second_timestamp},"101.5","104.0","100.0","102.0","8.0",{},"0",4,"0","0","0"]
                        ]"#,
                        first_timestamp + 59_999,
                        second_timestamp + 59_999,
                    );
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("REST response");
            }
            requests
        });

        let websocket_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("WebSocket listener");
        let websocket_base_url = format!(
            "ws://{}/ws",
            websocket_listener.local_addr().expect("WebSocket address")
        );
        let websocket_server = tokio::spawn(async move {
            let (first_connection, _) = websocket_listener
                .accept()
                .await
                .expect("first WebSocket connection");
            let mut first_socket = accept_hdr_async(
                first_connection,
                |_: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    Ok(response)
                },
            )
            .await
            .expect("first WebSocket handshake");
            first_socket
                .send(Message::Text(websocket_payload(
                    first_timestamp,
                    first_timestamp + 59_999,
                    "101.0",
                )))
                .await
                .expect("first candle");
            first_socket.close(None).await.expect("first close");

            let (second_connection, _) = websocket_listener
                .accept()
                .await
                .expect("second WebSocket connection");
            let mut second_socket = accept_hdr_async(
                second_connection,
                |_: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    Ok(response)
                },
            )
            .await
            .expect("second WebSocket handshake");
            second_socket
                .send(Message::Text(websocket_payload(
                    third_timestamp,
                    third_timestamp + 59_999,
                    "103.0",
                )))
                .await
                .expect("post-reconnect candle");

            assert!(matches!(
                timeout(Duration::from_secs(2), second_socket.next())
                    .await
                    .expect("subscription closes")
                    .expect("close frame is present")
                    .expect("valid close frame"),
                Message::Close(_)
            ));
        });

        let provider = BinanceSpotProvider::with_test_endpoints(base_url, websocket_base_url)
            .expect("provider");
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let subscription = provider
            .subscribe_candles(
                &candle_stream(),
                Arc::new(move |event| sender.send(event).expect("receiver remains open")),
            )
            .expect("subscription");

        let mut events = Vec::new();
        for _ in 0..4 {
            events.push(
                timeout(Duration::from_secs(2), receiver.recv())
                    .await
                    .expect("recovered event arrives")
                    .expect("subscription remains open"),
            );
        }
        drop(subscription);

        let requests = rest_server.await.expect("REST server task");
        websocket_server.await.expect("WebSocket server task");
        assert_eq!(requests.len(), 2);
        assert!(requests
            .iter()
            .all(|request| request.contains("startTime=1704067200000")));

        let closes = events
            .iter()
            .map(|event| match event {
                CandleEvent::Upsert { candle } => candle.close,
            })
            .collect::<Vec<_>>();
        assert_eq!(closes, vec![101.0, 101.5, 102.0, 103.0]);

        let mut store = CandleStore::new();
        for event in events {
            store.apply(event).expect("valid recovered candle");
        }
        let candles = store
            .candles(&candle_stream())
            .expect("recovered candle snapshot");
        assert_eq!(candles.len(), 3);
        assert_eq!(candles[0].close, 101.5);
        assert_eq!(candles[1].timestamp, second_timestamp);
        assert_eq!(candles[2].timestamp, third_timestamp);
    }

    #[test]
    fn websocket_event_must_match_its_subscription() {
        let payload = r#"{
            "e":"kline",
            "E":1704067259000,
            "s":"ETHUSDT",
            "k":{
                "t":1704067200000,
                "T":1704067259999,
                "s":"ETHUSDT",
                "i":"1m",
                "o":"100.0",
                "c":"101.5",
                "h":"102.0",
                "l":"99.0",
                "v":"10.5",
                "x":false
            }
        }"#;

        assert_eq!(
            normalize_stream_event(payload, &candle_stream()),
            Err(ProviderError::InvalidResponse(
                "kline symbol does not match its subscription".to_owned()
            ))
        );
    }

    #[test]
    fn rest_and_websocket_klines_share_the_same_canonical_shape() {
        let request = history_request();
        let history_candle = normalize_kline(
            (
                1_704_067_200_000,
                "100.0".to_owned(),
                "102.0".to_owned(),
                "99.0".to_owned(),
                "101.5".to_owned(),
                "10.5".to_owned(),
                1_704_067_259_999,
                "0".to_owned(),
                42,
                "0".to_owned(),
                "0".to_owned(),
                "0".to_owned(),
            ),
            &request,
            1_704_067_260_000,
        )
        .expect("history candle");
        let payload = r#"{
            "e":"kline",
            "E":1704067259000,
            "s":"BTCUSDT",
            "k":{
                "t":1704067200000,
                "T":1704067259999,
                "s":"BTCUSDT",
                "i":"1m",
                "o":"100.0",
                "h":"102.0",
                "l":"99.0",
                "c":"101.5",
                "v":"10.5",
                "x":true
            }
        }"#;
        let CandleEvent::Upsert {
            candle: stream_candle,
        } = normalize_stream_event(payload, &candle_stream()).expect("stream candle");

        assert_eq!(history_candle, stream_candle);
    }

    #[test]
    fn shared_normalizer_rejects_invalid_provider_timestamps() {
        let error = normalize_candle(
            BinanceKlineValues {
                timestamp: 1_704_067_200_000,
                close_time: 1_704_067_199_999,
                open: "100.0".to_owned(),
                high: "102.0".to_owned(),
                low: "99.0".to_owned(),
                close: "101.5".to_owned(),
                volume: "10.5".to_owned(),
            },
            &candle_stream(),
            true,
        );

        assert_eq!(
            error,
            Err(ProviderError::InvalidResponse(
                "closing timestamp precedes opening timestamp".to_owned()
            ))
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
