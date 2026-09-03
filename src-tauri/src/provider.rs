use std::{fmt, future::Future, pin::Pin, sync::Arc};

use tokio::sync::oneshot;

use crate::market_data::{
    CandleEvent, CandleHistoryRequest, CandleHistoryResponse, CandleStream, Instrument,
    IntervalDefinition, MarketDataValidationError,
};

pub type CandleEventHandler = Arc<dyn Fn(CandleEvent) + Send + Sync + 'static>;
pub type ProviderResult<T> = Result<T, ProviderError>;
pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = ProviderResult<T>> + Send + 'a>>;

/// Provider-neutral market-data boundary used by the backend.
pub trait MarketDataProvider: Send + Sync {
    fn id(&self) -> &str;
    fn list_symbols(&self) -> ProviderResult<Vec<Instrument>>;
    fn list_intervals(&self) -> ProviderResult<Vec<IntervalDefinition>>;
    fn get_candles<'a>(
        &'a self,
        request: &'a CandleHistoryRequest,
    ) -> ProviderFuture<'a, CandleHistoryResponse>;
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
    InvalidResponse(String),
    InvalidFixture(String),
    InvalidRequest(MarketDataValidationError),
    RateLimited { retry_after_seconds: Option<u64> },
    RequestLimitExceeded { requested: u32, maximum: u32 },
    RuntimeUnavailable,
    Transport(String),
    UnsupportedOperation(&'static str),
    UnknownInterval(String),
    UnknownProvider(String),
    UnknownSymbol(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidResponse(message) => {
                write!(formatter, "invalid provider response: {message}")
            }
            Self::InvalidFixture(message) => {
                write!(formatter, "invalid provider fixture: {message}")
            }
            Self::InvalidRequest(error) => {
                write!(formatter, "invalid market-data request: {error}")
            }
            Self::RateLimited {
                retry_after_seconds,
            } => match retry_after_seconds {
                Some(seconds) => write!(formatter, "provider rate limit; retry after {seconds}s"),
                None => write!(formatter, "provider rate limit"),
            },
            Self::RequestLimitExceeded { requested, maximum } => write!(
                formatter,
                "requested {requested} candles, but provider maximum is {maximum}"
            ),
            Self::RuntimeUnavailable => {
                write!(formatter, "a Tokio runtime is required to subscribe")
            }
            Self::Transport(message) => write!(formatter, "provider transport failed: {message}"),
            Self::UnsupportedOperation(operation) => {
                write!(
                    formatter,
                    "provider operation is not implemented: {operation}"
                )
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
