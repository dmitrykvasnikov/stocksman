use std::{fmt, sync::Arc};

use tokio::sync::oneshot;

use crate::market_data::{
    CandleEvent, CandleHistoryRequest, CandleHistoryResponse, CandleStream, Instrument,
    IntervalDefinition, MarketDataValidationError,
};

pub type CandleEventHandler = Arc<dyn Fn(CandleEvent) + Send + Sync + 'static>;
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Provider-neutral market-data boundary used by the backend.
pub trait MarketDataProvider: Send + Sync {
    fn id(&self) -> &str;
    fn list_symbols(&self) -> ProviderResult<Vec<Instrument>>;
    fn list_intervals(&self) -> ProviderResult<Vec<IntervalDefinition>>;
    fn get_candles(&self, request: &CandleHistoryRequest) -> ProviderResult<CandleHistoryResponse>;
    fn subscribe_candles(
        &self,
        request: &CandleStream,
        on_event: CandleEventHandler,
    ) -> ProviderResult<Unsubscribe>;
}

/// Cancels a live market-data subscription when called or dropped.
pub struct Unsubscribe {
    shutdown: Option<oneshot::Sender<()>>,
}

impl Unsubscribe {
    pub(crate) fn new(shutdown: oneshot::Sender<()>) -> Self {
        Self {
            shutdown: Some(shutdown),
        }
    }

    pub fn unsubscribe(mut self) {
        self.cancel();
    }

    fn cancel(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

impl Drop for Unsubscribe {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderError {
    InvalidFixture(String),
    InvalidRequest(MarketDataValidationError),
    RuntimeUnavailable,
    UnknownInterval(String),
    UnknownProvider(String),
    UnknownSymbol(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFixture(message) => {
                write!(formatter, "invalid provider fixture: {message}")
            }
            Self::InvalidRequest(error) => {
                write!(formatter, "invalid market-data request: {error}")
            }
            Self::RuntimeUnavailable => {
                write!(formatter, "a Tokio runtime is required to subscribe")
            }
            Self::UnknownInterval(interval) => write!(formatter, "unknown interval: {interval}"),
            Self::UnknownProvider(provider) => write!(formatter, "unknown provider: {provider}"),
            Self::UnknownSymbol(symbol) => write!(formatter, "unknown symbol: {symbol}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<MarketDataValidationError> for ProviderError {
    fn from(error: MarketDataValidationError) -> Self {
        Self::InvalidRequest(error)
    }
}
