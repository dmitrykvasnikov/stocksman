use std::{collections::HashSet, sync::Arc, time::Duration};

use tokio::{runtime::Handle, sync::oneshot, time::sleep};

use crate::{
    market_data::{
        Candle, CandleEvent, CandleHistoryRequest, CandleHistoryResponse, CandleStream, Instrument,
        IntervalDefinition, IntervalUnit,
    },
    provider::{
        CandleEventHandler, MarketDataProvider, ProviderError, ProviderResult, Unsubscribe,
    },
};

pub const MOCK_PROVIDER_ID: &str = "mock";
const SAMPLE_START_TIMESTAMP: i64 = 1_704_067_200_000;
const SAMPLE_CANDLE_COUNT: usize = 120;

/// An offline provider backed by an immutable, repeatable sequence of candles.
///
/// Historical queries are timestamp ordered. Subscriptions emit the original
/// fixture order so tests can model revisions, duplicates, and out-of-order data.
#[derive(Clone)]
pub struct MockReplayProvider {
    instruments: Arc<[Instrument]>,
    intervals: Arc<[IntervalDefinition]>,
    replay: Arc<[Candle]>,
    replay_cadence: Duration,
}

impl MockReplayProvider {
    pub fn new(
        instruments: Vec<Instrument>,
        intervals: Vec<IntervalDefinition>,
        replay: Vec<Candle>,
        replay_cadence: Duration,
    ) -> ProviderResult<Self> {
        validate_fixture(&instruments, &intervals, &replay)?;

        Ok(Self {
            instruments: instruments.into(),
            intervals: intervals.into(),
            replay: replay.into(),
            replay_cadence,
        })
    }

    /// Returns the deterministic offline dataset used for local development.
    pub fn sample(replay_cadence: Duration) -> Self {
        let instruments = sample_instruments();
        let intervals = sample_intervals();
        let mut candles =
            Vec::with_capacity(instruments.len() * intervals.len() * SAMPLE_CANDLE_COUNT);

        for (instrument_index, instrument) in instruments.iter().enumerate() {
            for interval in &intervals {
                candles.extend(sample_candles(instrument, interval, instrument_index));
            }
        }

        Self::new(instruments, intervals, candles, replay_cadence)
            .expect("built-in mock market data must be valid")
    }

    fn validate_stream(&self, stream: &CandleStream) -> ProviderResult<()> {
        stream.validate()?;
        if stream.provider != MOCK_PROVIDER_ID {
            return Err(ProviderError::UnknownProvider(stream.provider.clone()));
        }
        if !self
            .instruments
            .iter()
            .any(|instrument| instrument.symbol == stream.symbol)
        {
            return Err(ProviderError::UnknownSymbol(stream.symbol.clone()));
        }
        if !self
            .intervals
            .iter()
            .any(|interval| interval.id == stream.interval)
        {
            return Err(ProviderError::UnknownInterval(stream.interval.clone()));
        }
        Ok(())
    }
}

impl MarketDataProvider for MockReplayProvider {
    fn id(&self) -> &str {
        MOCK_PROVIDER_ID
    }

    fn list_symbols(&self) -> ProviderResult<Vec<Instrument>> {
        Ok(self.instruments.to_vec())
    }

    fn list_intervals(&self) -> ProviderResult<Vec<IntervalDefinition>> {
        Ok(self.intervals.to_vec())
    }

    fn get_candles(&self, request: &CandleHistoryRequest) -> ProviderResult<CandleHistoryResponse> {
        request.validate()?;
        self.validate_stream(&CandleStream {
            provider: request.provider.clone(),
            symbol: request.symbol.clone(),
            interval: request.interval.clone(),
        })?;

        let mut candles = self
            .replay
            .iter()
            .filter(|candle| {
                candle.symbol == request.symbol
                    && candle.interval == request.interval
                    && request
                        .start_timestamp
                        .map_or(true, |start| candle.timestamp >= start)
                    && request
                        .end_timestamp
                        .map_or(true, |end| candle.timestamp <= end)
            })
            .cloned()
            .collect::<Vec<_>>();
        candles.sort_by_key(|candle| candle.timestamp);

        if let Some(limit) = request.limit {
            let keep = limit as usize;
            if candles.len() > keep {
                candles.drain(..candles.len() - keep);
            }
        }

        Ok(CandleHistoryResponse { candles })
    }

    fn subscribe_candles(
        &self,
        request: &CandleStream,
        on_event: CandleEventHandler,
    ) -> ProviderResult<Unsubscribe> {
        self.validate_stream(request)?;
        let runtime = Handle::try_current().map_err(|_| ProviderError::RuntimeUnavailable)?;
        let replay = self
            .replay
            .iter()
            .filter(|candle| candle.symbol == request.symbol && candle.interval == request.interval)
            .cloned()
            .collect::<Vec<_>>();
        let cadence = self.replay_cadence;
        let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();

        runtime.spawn(async move {
            for candle in replay {
                tokio::select! {
                    _ = &mut shutdown_receiver => return,
                    _ = sleep(cadence) => on_event(CandleEvent::Upsert { candle }),
                }
            }
        });

        Ok(Unsubscribe::new(shutdown_sender))
    }
}

fn validate_fixture(
    instruments: &[Instrument],
    intervals: &[IntervalDefinition],
    replay: &[Candle],
) -> ProviderResult<()> {
    if instruments.is_empty() {
        return Err(invalid_fixture("at least one instrument is required"));
    }
    if intervals.is_empty() {
        return Err(invalid_fixture("at least one interval is required"));
    }

    let mut symbols = HashSet::new();
    for instrument in instruments {
        instrument
            .validate()
            .map_err(|error| invalid_fixture(error.to_string()))?;
        if instrument.provider != MOCK_PROVIDER_ID {
            return Err(invalid_fixture(format!(
                "instrument {} belongs to provider {}",
                instrument.symbol, instrument.provider
            )));
        }
        if !symbols.insert(instrument.symbol.as_str()) {
            return Err(invalid_fixture(format!(
                "duplicate instrument {}",
                instrument.symbol
            )));
        }
    }

    let mut interval_ids = HashSet::new();
    for interval in intervals {
        interval
            .validate()
            .map_err(|error| invalid_fixture(error.to_string()))?;
        if !interval_ids.insert(interval.id.as_str()) {
            return Err(invalid_fixture(format!(
                "duplicate interval {}",
                interval.id
            )));
        }
    }

    for candle in replay {
        candle
            .validate()
            .map_err(|error| invalid_fixture(error.to_string()))?;
        if candle.provider != MOCK_PROVIDER_ID {
            return Err(invalid_fixture(format!(
                "candle belongs to provider {}",
                candle.provider
            )));
        }
        if !symbols.contains(candle.symbol.as_str()) {
            return Err(invalid_fixture(format!(
                "candle uses unknown symbol {}",
                candle.symbol
            )));
        }
        if !interval_ids.contains(candle.interval.as_str()) {
            return Err(invalid_fixture(format!(
                "candle uses unknown interval {}",
                candle.interval
            )));
        }
    }

    Ok(())
}

fn invalid_fixture(message: impl Into<String>) -> ProviderError {
    ProviderError::InvalidFixture(message.into())
}

fn sample_instruments() -> Vec<Instrument> {
    ["BTC", "ETH", "BNB", "SOL", "XRP"]
        .into_iter()
        .map(|base_asset| Instrument {
            provider: MOCK_PROVIDER_ID.to_owned(),
            symbol: format!("{base_asset}USDT"),
            base_asset: base_asset.to_owned(),
            quote_asset: "USDT".to_owned(),
        })
        .collect()
}

fn sample_intervals() -> Vec<IntervalDefinition> {
    vec![
        IntervalDefinition {
            id: "1m".to_owned(),
            label: "1 minute".to_owned(),
            amount: 1,
            unit: IntervalUnit::Minute,
        },
        IntervalDefinition {
            id: "5m".to_owned(),
            label: "5 minutes".to_owned(),
            amount: 5,
            unit: IntervalUnit::Minute,
        },
        IntervalDefinition {
            id: "1h".to_owned(),
            label: "1 hour".to_owned(),
            amount: 1,
            unit: IntervalUnit::Hour,
        },
        IntervalDefinition {
            id: "1d".to_owned(),
            label: "1 day".to_owned(),
            amount: 1,
            unit: IntervalUnit::Day,
        },
    ]
}

fn sample_candles(
    instrument: &Instrument,
    interval: &IntervalDefinition,
    instrument_index: usize,
) -> Vec<Candle> {
    let interval_millis = match interval.id.as_str() {
        "1m" => 60_000,
        "5m" => 300_000,
        "1h" => 3_600_000,
        "1d" => 86_400_000,
        _ => unreachable!("sample intervals are fixed"),
    };
    let base_prices = [42_000.0, 2_300.0, 310.0, 105.0, 0.62];
    let price_scale = base_prices[instrument_index] / 1_000.0;
    let mut previous_close = base_prices[instrument_index];

    (0..SAMPLE_CANDLE_COUNT)
        .map(|index| {
            let open = previous_close;
            let wave = (((index + instrument_index * 3) % 11) as f64 - 5.0) * price_scale;
            let close = (open + wave).max(price_scale);
            let wick = price_scale * (1.0 + (index % 3) as f64 * 0.25);
            previous_close = close;

            Candle {
                provider: MOCK_PROVIDER_ID.to_owned(),
                symbol: instrument.symbol.clone(),
                interval: interval.id.clone(),
                timestamp: SAMPLE_START_TIMESTAMP + index as i64 * interval_millis,
                open,
                high: open.max(close) + wick,
                low: (open.min(close) - wick).max(price_scale / 10.0),
                close,
                volume: 25.0 + ((index * 17 + instrument_index * 13) % 80) as f64,
                closed: index + 1 < SAMPLE_CANDLE_COUNT,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use tokio::{sync::mpsc, time::timeout};

    use super::*;

    fn request(symbol: &str, interval: &str) -> CandleHistoryRequest {
        CandleHistoryRequest {
            provider: MOCK_PROVIDER_ID.to_owned(),
            symbol: symbol.to_owned(),
            interval: interval.to_owned(),
            start_timestamp: None,
            end_timestamp: None,
            limit: None,
        }
    }

    fn instrument() -> Instrument {
        Instrument {
            provider: MOCK_PROVIDER_ID.to_owned(),
            symbol: "BTCUSDT".to_owned(),
            base_asset: "BTC".to_owned(),
            quote_asset: "USDT".to_owned(),
        }
    }

    fn interval() -> IntervalDefinition {
        IntervalDefinition {
            id: "1m".to_owned(),
            label: "1 minute".to_owned(),
            amount: 1,
            unit: IntervalUnit::Minute,
        }
    }

    fn candle(timestamp: i64, close: f64) -> Candle {
        Candle {
            provider: MOCK_PROVIDER_ID.to_owned(),
            symbol: "BTCUSDT".to_owned(),
            interval: "1m".to_owned(),
            timestamp,
            open: close,
            high: close + 1.0,
            low: (close - 0.5).max(0.1),
            close,
            volume: 10.0,
            closed: true,
        }
    }

    #[test]
    fn sample_metadata_and_history_are_repeatable() {
        let first = MockReplayProvider::sample(Duration::ZERO);
        let second = MockReplayProvider::sample(Duration::ZERO);
        let mut latest = request("BTCUSDT", "1m");
        latest.limit = Some(3);

        assert_eq!(first.id(), MOCK_PROVIDER_ID);
        assert_eq!(first.list_symbols().expect("symbols").len(), 5);
        assert_eq!(first.list_intervals().expect("intervals").len(), 4);
        assert_eq!(
            first.get_candles(&latest).expect("first history"),
            second.get_candles(&latest).expect("second history")
        );
        assert_eq!(
            first
                .get_candles(&latest)
                .expect("limited history")
                .candles
                .len(),
            3
        );
    }

    #[test]
    fn history_is_ordered_and_uses_inclusive_boundaries() {
        let provider = MockReplayProvider::new(
            vec![instrument()],
            vec![interval()],
            vec![
                candle(180_000, 3.0),
                candle(60_000, 1.0),
                candle(120_000, 2.0),
            ],
            Duration::ZERO,
        )
        .expect("provider");
        let mut history_request = request("BTCUSDT", "1m");
        history_request.start_timestamp = Some(60_000);
        history_request.end_timestamp = Some(120_000);

        let timestamps = provider
            .get_candles(&history_request)
            .expect("history")
            .candles
            .into_iter()
            .map(|candle| candle.timestamp)
            .collect::<Vec<_>>();

        assert_eq!(timestamps, vec![60_000, 120_000]);
    }

    #[tokio::test]
    async fn replay_preserves_fixture_order_and_duplicate_revisions() {
        let provider = MockReplayProvider::new(
            vec![instrument()],
            vec![interval()],
            vec![
                candle(120_000, 2.0),
                candle(60_000, 1.0),
                candle(120_000, 2.5),
            ],
            Duration::ZERO,
        )
        .expect("provider");
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let _subscription = provider
            .subscribe_candles(
                &CandleStream {
                    provider: MOCK_PROVIDER_ID.to_owned(),
                    symbol: "BTCUSDT".to_owned(),
                    interval: "1m".to_owned(),
                },
                Arc::new(move |event| {
                    sender.send(event).expect("replay receiver remains open");
                }),
            )
            .expect("subscription");

        let mut observed = Vec::new();
        for _ in 0..3 {
            let event = timeout(Duration::from_secs(1), receiver.recv())
                .await
                .expect("event arrives")
                .expect("replay remains open");
            let CandleEvent::Upsert { candle } = event;
            observed.push((candle.timestamp, candle.close));
        }

        assert_eq!(
            observed,
            vec![(120_000, 2.0), (60_000, 1.0), (120_000, 2.5)]
        );
    }

    #[tokio::test]
    async fn dropping_a_subscription_stops_replay() {
        let provider = MockReplayProvider::new(
            vec![instrument()],
            vec![interval()],
            vec![candle(60_000, 1.0)],
            Duration::from_secs(30),
        )
        .expect("provider");
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let subscription = provider
            .subscribe_candles(
                &CandleStream {
                    provider: MOCK_PROVIDER_ID.to_owned(),
                    symbol: "BTCUSDT".to_owned(),
                    interval: "1m".to_owned(),
                },
                Arc::new(move |event| {
                    sender.send(event).expect("replay receiver remains open");
                }),
            )
            .expect("subscription");

        drop(subscription);

        assert!(timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect("replay task stops promptly")
            .is_none());
    }

    #[test]
    fn fixtures_reject_unknown_stream_metadata() {
        let error = match MockReplayProvider::new(
            vec![instrument()],
            vec![interval()],
            vec![Candle {
                symbol: "ETHUSDT".to_owned(),
                ..candle(60_000, 1.0)
            }],
            Duration::ZERO,
        ) {
            Ok(_) => panic!("unknown fixture symbol must fail"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ProviderError::InvalidFixture("candle uses unknown symbol ETHUSDT".to_owned())
        );
    }
}
