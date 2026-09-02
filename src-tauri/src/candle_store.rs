use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    fmt,
};

use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};

use crate::market_data::{
    Candle, CandleEvent, CandleStream, IntervalDefinition, IntervalUnit, MarketDataValidationError,
    UtcTimestamp,
};

/// A missing section between two observed candle opening times.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandleGap {
    pub previous_timestamp: UtcTimestamp,
    pub expected_timestamp: UtcTimestamp,
    pub next_timestamp: UtcTimestamp,
    pub missing_candles: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpsertOutcome {
    Inserted,
    Revised,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpsertSummary {
    pub inserted: usize,
    pub revised: usize,
    pub duplicates: usize,
}

/// Provider-neutral, in-memory cache of canonical candle series.
///
/// A candle is identified by provider, symbol, interval, and opening timestamp.
/// The most recently received value wins when that identity is revised. Exact
/// repeats are ignored, and BTreeMap storage makes every snapshot deterministic
/// regardless of delivery order.
#[derive(Debug, Default)]
pub struct CandleStore {
    series: HashMap<CandleStream, BTreeMap<UtcTimestamp, Candle>>,
}

impl CandleStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, candle: Candle) -> Result<UpsertOutcome, CandleStoreError> {
        candle.validate().map_err(CandleStoreError::InvalidData)?;
        Ok(self.upsert_validated(candle))
    }

    pub fn apply(&mut self, event: CandleEvent) -> Result<UpsertOutcome, CandleStoreError> {
        match event {
            CandleEvent::Upsert { candle } => self.upsert(candle),
        }
    }

    /// Atomically validates and applies a group of candles.
    ///
    /// If any candle is invalid, none of the group is stored.
    pub fn upsert_all(
        &mut self,
        candles: impl IntoIterator<Item = Candle>,
    ) -> Result<UpsertSummary, CandleStoreError> {
        let candles = candles.into_iter().collect::<Vec<_>>();
        for candle in &candles {
            candle.validate().map_err(CandleStoreError::InvalidData)?;
        }

        let mut summary = UpsertSummary::default();
        for candle in candles {
            match self.upsert_validated(candle) {
                UpsertOutcome::Inserted => summary.inserted += 1,
                UpsertOutcome::Revised => summary.revised += 1,
                UpsertOutcome::Duplicate => summary.duplicates += 1,
            }
        }
        Ok(summary)
    }

    /// Returns an immutable, timestamp-ordered snapshot of one candle series.
    pub fn candles(&self, stream: &CandleStream) -> Result<Vec<Candle>, CandleStoreError> {
        stream.validate().map_err(CandleStoreError::InvalidData)?;
        Ok(self
            .series
            .get(stream)
            .map(|candles| candles.values().cloned().collect())
            .unwrap_or_default())
    }

    pub fn len(&self, stream: &CandleStream) -> Result<usize, CandleStoreError> {
        stream.validate().map_err(CandleStoreError::InvalidData)?;
        Ok(self.series.get(stream).map_or(0, BTreeMap::len))
    }

    pub fn is_empty(&self) -> bool {
        self.series.values().all(BTreeMap::is_empty)
    }

    /// Finds missing interval openings between adjacent stored candles.
    ///
    /// Fixed-size intervals use checked millisecond arithmetic. Month intervals
    /// advance by UTC calendar months so February and leap years retain their
    /// real lengths. A timestamp that is not on the expected cadence is bounded
    /// by the same gap and does not get silently rounded.
    pub fn gaps(
        &self,
        stream: &CandleStream,
        interval: &IntervalDefinition,
    ) -> Result<Vec<CandleGap>, CandleStoreError> {
        stream.validate().map_err(CandleStoreError::InvalidData)?;
        interval.validate().map_err(CandleStoreError::InvalidData)?;
        if stream.interval != interval.id {
            return Err(CandleStoreError::IntervalMismatch {
                stream_interval: stream.interval.clone(),
                definition_interval: interval.id.clone(),
            });
        }

        let Some(candles) = self.series.get(stream) else {
            return Ok(Vec::new());
        };
        let timestamps = candles.keys().copied().collect::<Vec<_>>();
        let mut gaps = Vec::new();

        for pair in timestamps.windows(2) {
            let previous_timestamp = pair[0];
            let next_timestamp = pair[1];
            let expected_timestamp = advance_timestamp(previous_timestamp, interval)?;
            if next_timestamp <= expected_timestamp {
                continue;
            }

            let missing_candles = count_missing(expected_timestamp, next_timestamp, interval)?;
            gaps.push(CandleGap {
                previous_timestamp,
                expected_timestamp,
                next_timestamp,
                missing_candles,
            });
        }

        Ok(gaps)
    }

    fn upsert_validated(&mut self, candle: Candle) -> UpsertOutcome {
        let stream = CandleStream {
            provider: candle.provider.clone(),
            symbol: candle.symbol.clone(),
            interval: candle.interval.clone(),
        };
        let candles = self.series.entry(stream).or_default();

        match candles.entry(candle.timestamp) {
            Entry::Vacant(entry) => {
                entry.insert(candle);
                UpsertOutcome::Inserted
            }
            Entry::Occupied(mut entry) if entry.get() != &candle => {
                entry.insert(candle);
                UpsertOutcome::Revised
            }
            Entry::Occupied(_) => UpsertOutcome::Duplicate,
        }
    }
}

fn count_missing(
    expected_timestamp: UtcTimestamp,
    next_timestamp: UtcTimestamp,
    interval: &IntervalDefinition,
) -> Result<u64, CandleStoreError> {
    if interval.unit != IntervalUnit::Month {
        let duration = fixed_interval_millis(interval)?;
        let distance = next_timestamp - expected_timestamp;
        return Ok(1 + (distance - 1) as u64 / duration as u64);
    }

    let mut missing = 0_u64;
    let mut cursor = expected_timestamp;
    while cursor < next_timestamp {
        missing = missing
            .checked_add(1)
            .ok_or(CandleStoreError::TimestampOverflow(cursor))?;
        cursor = advance_timestamp(cursor, interval)?;
    }
    Ok(missing)
}

fn advance_timestamp(
    timestamp: UtcTimestamp,
    interval: &IntervalDefinition,
) -> Result<UtcTimestamp, CandleStoreError> {
    if interval.unit == IntervalUnit::Month {
        let date_time = DateTime::<Utc>::from_timestamp_millis(timestamp)
            .ok_or(CandleStoreError::TimestampOutOfRange(timestamp))?;
        return date_time
            .checked_add_months(Months::new(interval.amount))
            .map(|next| next.timestamp_millis())
            .ok_or(CandleStoreError::TimestampOverflow(timestamp));
    }

    timestamp
        .checked_add(fixed_interval_millis(interval)?)
        .ok_or(CandleStoreError::TimestampOverflow(timestamp))
}

fn fixed_interval_millis(interval: &IntervalDefinition) -> Result<UtcTimestamp, CandleStoreError> {
    let unit_millis = match interval.unit {
        IntervalUnit::Second => 1_000_i64,
        IntervalUnit::Minute => 60_000,
        IntervalUnit::Hour => 3_600_000,
        IntervalUnit::Day => 86_400_000,
        IntervalUnit::Week => 604_800_000,
        IntervalUnit::Month => return Err(CandleStoreError::InvalidMonthCalculation),
    };

    unit_millis
        .checked_mul(i64::from(interval.amount))
        .ok_or(CandleStoreError::InvalidIntervalDuration)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandleStoreError {
    InvalidData(MarketDataValidationError),
    IntervalMismatch {
        stream_interval: String,
        definition_interval: String,
    },
    InvalidIntervalDuration,
    InvalidMonthCalculation,
    TimestampOutOfRange(UtcTimestamp),
    TimestampOverflow(UtcTimestamp),
}

impl fmt::Display for CandleStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidData(error) => write!(formatter, "invalid candle-store data: {error}"),
            Self::IntervalMismatch {
                stream_interval,
                definition_interval,
            } => write!(
                formatter,
                "stream interval {stream_interval} does not match definition {definition_interval}"
            ),
            Self::InvalidIntervalDuration => write!(formatter, "interval duration is too large"),
            Self::InvalidMonthCalculation => {
                write!(formatter, "month intervals require calendar arithmetic")
            }
            Self::TimestampOutOfRange(timestamp) => {
                write!(
                    formatter,
                    "timestamp is outside the supported UTC range: {timestamp}"
                )
            }
            Self::TimestampOverflow(timestamp) => {
                write!(
                    formatter,
                    "interval after timestamp would overflow: {timestamp}"
                )
            }
        }
    }
}

impl std::error::Error for CandleStoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(interval: &str) -> CandleStream {
        CandleStream {
            provider: "mock".to_owned(),
            symbol: "BTCUSDT".to_owned(),
            interval: interval.to_owned(),
        }
    }

    fn interval(id: &str, amount: u32, unit: IntervalUnit) -> IntervalDefinition {
        IntervalDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            amount,
            unit,
        }
    }

    fn candle(timestamp: i64, close: f64, interval: &str) -> Candle {
        Candle {
            provider: "mock".to_owned(),
            symbol: "BTCUSDT".to_owned(),
            interval: interval.to_owned(),
            timestamp,
            open: close,
            high: close + 1.0,
            low: (close - 1.0).max(0.1),
            close,
            volume: 10.0,
            closed: true,
        }
    }

    #[test]
    fn out_of_order_duplicates_and_revisions_produce_one_ordered_series() {
        let mut store = CandleStore::new();

        assert_eq!(
            store.upsert(candle(180_000, 3.0, "1m")),
            Ok(UpsertOutcome::Inserted)
        );
        assert_eq!(
            store.upsert(candle(60_000, 1.0, "1m")),
            Ok(UpsertOutcome::Inserted)
        );
        assert_eq!(
            store.upsert(candle(180_000, 3.0, "1m")),
            Ok(UpsertOutcome::Duplicate)
        );
        assert_eq!(
            store.upsert(candle(180_000, 3.5, "1m")),
            Ok(UpsertOutcome::Revised)
        );
        assert_eq!(
            store.upsert(candle(120_000, 2.0, "1m")),
            Ok(UpsertOutcome::Inserted)
        );

        let observed = store
            .candles(&stream("1m"))
            .expect("ordered snapshot")
            .into_iter()
            .map(|candle| (candle.timestamp, candle.close))
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            vec![(60_000, 1.0), (120_000, 2.0), (180_000, 3.5)]
        );
    }

    #[test]
    fn batch_upserts_are_validated_before_the_store_changes() {
        let mut store = CandleStore::new();
        let mut invalid = candle(120_000, 2.0, "1m");
        invalid.volume = -1.0;

        let result = store.upsert_all(vec![candle(60_000, 1.0, "1m"), invalid]);

        assert_eq!(
            result,
            Err(CandleStoreError::InvalidData(
                MarketDataValidationError::NegativeValue("volume")
            ))
        );
        assert!(store.is_empty());
    }

    #[test]
    fn gap_detection_reports_each_missing_fixed_interval_opening() {
        let mut store = CandleStore::new();
        store
            .upsert_all(vec![
                candle(60_000, 1.0, "1m"),
                candle(120_000, 2.0, "1m"),
                candle(300_000, 5.0, "1m"),
            ])
            .expect("valid candles");

        assert_eq!(
            store.gaps(&stream("1m"), &interval("1m", 1, IntervalUnit::Minute)),
            Ok(vec![CandleGap {
                previous_timestamp: 120_000,
                expected_timestamp: 180_000,
                next_timestamp: 300_000,
                missing_candles: 2,
            }])
        );
    }

    #[test]
    fn calendar_months_use_real_month_boundaries() {
        let january = 1_704_067_200_000;
        let february = 1_706_745_600_000;
        let march = 1_709_251_200_000;
        let april = 1_711_929_600_000;
        let monthly = interval("1M", 1, IntervalUnit::Month);
        let mut contiguous = CandleStore::new();
        contiguous
            .upsert_all(vec![
                candle(january, 1.0, "1M"),
                candle(february, 2.0, "1M"),
                candle(march, 3.0, "1M"),
            ])
            .expect("valid monthly candles");

        assert_eq!(contiguous.gaps(&stream("1M"), &monthly), Ok(Vec::new()));

        let mut with_gap = CandleStore::new();
        with_gap
            .upsert_all(vec![candle(january, 1.0, "1M"), candle(april, 4.0, "1M")])
            .expect("valid monthly candles");
        assert_eq!(
            with_gap.gaps(&stream("1M"), &monthly),
            Ok(vec![CandleGap {
                previous_timestamp: january,
                expected_timestamp: february,
                next_timestamp: april,
                missing_candles: 2,
            }])
        );
    }

    #[test]
    fn series_and_interval_definitions_cannot_be_mixed() {
        let store = CandleStore::new();

        assert_eq!(
            store.gaps(&stream("1m"), &interval("5m", 5, IntervalUnit::Minute)),
            Err(CandleStoreError::IntervalMismatch {
                stream_interval: "1m".to_owned(),
                definition_interval: "5m".to_owned(),
            })
        );
    }
}
