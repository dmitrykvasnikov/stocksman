use std::fmt;

use serde::{Deserialize, Serialize};

/// Milliseconds since the Unix epoch in UTC.
pub type UtcTimestamp = i64;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    pub provider: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntervalUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntervalDefinition {
    /// Provider-neutral identifier used by requests and candles.
    pub id: String,
    /// Human-readable text for interval selectors.
    pub label: String,
    pub amount: u32,
    pub unit: IntervalUnit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candle {
    pub provider: String,
    pub symbol: String,
    pub interval: String,
    pub timestamp: UtcTimestamp,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub closed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandleStream {
    pub provider: String,
    pub symbol: String,
    pub interval: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandleHistoryRequest {
    pub provider: String,
    pub symbol: String,
    pub interval: String,
    /// Inclusive opening-time boundary.
    pub start_timestamp: Option<UtcTimestamp>,
    /// Inclusive opening-time boundary.
    pub end_timestamp: Option<UtcTimestamp>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandleHistoryResponse {
    pub candles: Vec<Candle>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MarketDataCatalog {
    pub instruments: Vec<Instrument>,
    pub intervals: Vec<IntervalDefinition>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CandleEvent {
    Upsert { candle: Candle },
}

impl Instrument {
    pub fn validate(&self) -> Result<(), MarketDataValidationError> {
        validate_identifier("provider", &self.provider)?;
        validate_identifier("symbol", &self.symbol)?;
        validate_identifier("base_asset", &self.base_asset)?;
        validate_identifier("quote_asset", &self.quote_asset)
    }
}

impl IntervalDefinition {
    pub fn validate(&self) -> Result<(), MarketDataValidationError> {
        validate_identifier("id", &self.id)?;
        validate_identifier("label", &self.label)?;
        if self.amount == 0 {
            return Err(MarketDataValidationError::ZeroValue("amount"));
        }
        Ok(())
    }
}

impl Candle {
    pub fn validate(&self) -> Result<(), MarketDataValidationError> {
        validate_stream(&self.provider, &self.symbol, &self.interval)?;
        validate_timestamp("timestamp", self.timestamp)?;
        validate_positive_number("open", self.open)?;
        validate_positive_number("high", self.high)?;
        validate_positive_number("low", self.low)?;
        validate_positive_number("close", self.close)?;
        validate_non_negative_number("volume", self.volume)?;

        if self.high < self.open.max(self.close) || self.low > self.open.min(self.close) {
            return Err(MarketDataValidationError::InvalidPriceRange);
        }

        Ok(())
    }
}

impl CandleStream {
    pub fn validate(&self) -> Result<(), MarketDataValidationError> {
        validate_stream(&self.provider, &self.symbol, &self.interval)
    }
}

impl CandleHistoryRequest {
    pub fn validate(&self) -> Result<(), MarketDataValidationError> {
        validate_stream(&self.provider, &self.symbol, &self.interval)?;

        if let Some(timestamp) = self.start_timestamp {
            validate_timestamp("start_timestamp", timestamp)?;
        }
        if let Some(timestamp) = self.end_timestamp {
            validate_timestamp("end_timestamp", timestamp)?;
        }
        if self
            .start_timestamp
            .zip(self.end_timestamp)
            .is_some_and(|(start, end)| start > end)
        {
            return Err(MarketDataValidationError::InvalidTimeRange);
        }
        if self.limit == Some(0) {
            return Err(MarketDataValidationError::ZeroValue("limit"));
        }

        Ok(())
    }
}

fn validate_stream(
    provider: &str,
    symbol: &str,
    interval: &str,
) -> Result<(), MarketDataValidationError> {
    validate_identifier("provider", provider)?;
    validate_identifier("symbol", symbol)?;
    validate_identifier("interval", interval)
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), MarketDataValidationError> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(MarketDataValidationError::InvalidIdentifier(field));
    }
    Ok(())
}

fn validate_timestamp(
    field: &'static str,
    value: UtcTimestamp,
) -> Result<(), MarketDataValidationError> {
    if value < 0 {
        return Err(MarketDataValidationError::NegativeValue(field));
    }
    Ok(())
}

fn validate_positive_number(
    field: &'static str,
    value: f64,
) -> Result<(), MarketDataValidationError> {
    if !value.is_finite() {
        return Err(MarketDataValidationError::NonFiniteNumber(field));
    }
    if value <= 0.0 {
        return Err(MarketDataValidationError::NonPositiveNumber(field));
    }
    Ok(())
}

fn validate_non_negative_number(
    field: &'static str,
    value: f64,
) -> Result<(), MarketDataValidationError> {
    if !value.is_finite() {
        return Err(MarketDataValidationError::NonFiniteNumber(field));
    }
    if value < 0.0 {
        return Err(MarketDataValidationError::NegativeValue(field));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketDataValidationError {
    InvalidIdentifier(&'static str),
    InvalidPriceRange,
    InvalidTimeRange,
    NegativeValue(&'static str),
    NonFiniteNumber(&'static str),
    NonPositiveNumber(&'static str),
    ZeroValue(&'static str),
}

impl fmt::Display for MarketDataValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(field) => write!(formatter, "invalid identifier: {field}"),
            Self::InvalidPriceRange => write!(formatter, "candle prices are outside its range"),
            Self::InvalidTimeRange => write!(formatter, "start timestamp is after end timestamp"),
            Self::NegativeValue(field) => write!(formatter, "negative value: {field}"),
            Self::NonFiniteNumber(field) => write!(formatter, "non-finite number: {field}"),
            Self::NonPositiveNumber(field) => write!(formatter, "non-positive number: {field}"),
            Self::ZeroValue(field) => write!(formatter, "zero value: {field}"),
        }
    }
}

impl std::error::Error for MarketDataValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle() -> Candle {
        Candle {
            provider: "mock".to_owned(),
            symbol: "BTCUSDT".to_owned(),
            interval: "1m".to_owned(),
            timestamp: 1_725_000_000_000,
            open: 60_000.0,
            high: 60_200.0,
            low: 59_900.0,
            close: 60_100.0,
            volume: 12.5,
            closed: true,
        }
    }

    #[test]
    fn candle_has_a_stable_provider_neutral_json_shape() {
        assert_eq!(
            serde_json::to_value(candle()).expect("serialize candle"),
            serde_json::json!({
                "provider": "mock",
                "symbol": "BTCUSDT",
                "interval": "1m",
                "timestamp": 1_725_000_000_000_i64,
                "open": 60_000.0,
                "high": 60_200.0,
                "low": 59_900.0,
                "close": 60_100.0,
                "volume": 12.5,
                "closed": true
            })
        );
    }

    #[test]
    fn valid_candles_and_zero_volume_are_accepted() {
        let mut value = candle();
        value.volume = 0.0;

        assert_eq!(value.validate(), Ok(()));
    }

    #[test]
    fn impossible_price_ranges_are_rejected() {
        let mut value = candle();
        value.high = value.open - 1.0;

        assert_eq!(
            value.validate(),
            Err(MarketDataValidationError::InvalidPriceRange)
        );
    }

    #[test]
    fn non_finite_numbers_are_rejected_before_the_api_boundary() {
        let mut value = candle();
        value.close = f64::NAN;

        assert_eq!(
            value.validate(),
            Err(MarketDataValidationError::NonFiniteNumber("close"))
        );
    }

    #[test]
    fn history_request_boundaries_are_inclusive_and_ordered() {
        let request = CandleHistoryRequest {
            provider: "mock".to_owned(),
            symbol: "BTCUSDT".to_owned(),
            interval: "1m".to_owned(),
            start_timestamp: Some(1_000),
            end_timestamp: Some(1_000),
            limit: Some(1),
        };
        assert_eq!(request.validate(), Ok(()));

        let invalid = CandleHistoryRequest {
            start_timestamp: Some(1_001),
            ..request
        };
        assert_eq!(
            invalid.validate(),
            Err(MarketDataValidationError::InvalidTimeRange)
        );
    }

    #[test]
    fn candle_events_use_a_discriminated_shape() {
        let event = CandleEvent::Upsert { candle: candle() };
        let value = serde_json::to_value(event).expect("serialize event");

        assert_eq!(value["kind"], "upsert");
        assert_eq!(value["candle"]["interval"], "1m");
    }
}
