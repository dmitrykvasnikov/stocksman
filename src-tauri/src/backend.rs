use std::{
    collections::HashMap,
    io::{self, Write},
    net::Ipv4Addr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::{
    database::{ConfigurationStore, UserConfiguration},
    market_data::{CandleHistoryRequest, MarketDataCatalog},
    provider::{MarketDataProvider, ProviderError, ProviderResult},
    providers::{
        binance::{BinanceSpotProvider, BINANCE_PROVIDER_ID},
        mock::{MockReplayProvider, MOCK_PROVIDER_ID},
    },
};

const MAX_REQUEST_BYTES: usize = 65_536;

#[derive(Serialize)]
struct BackendAnnouncement {
    port: u16,
}

struct HttpRequest {
    method: String,
    path: String,
    origin: Option<String>,
    body: Vec<u8>,
}

pub fn run(database_path: PathBuf) -> io::Result<()> {
    let store = ConfigurationStore::open(&database_path).map_err(io::Error::other)?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
            let address = listener.local_addr()?;
            let announcement = BackendAnnouncement {
                port: address.port(),
            };

            {
                let mut stdout = io::stdout().lock();
                writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&announcement).map_err(io::Error::other)?
                )?;
                stdout.flush()?;
            }

            serve(listener, Arc::new(store)).await
        })
}

async fn serve(listener: TcpListener, store: Arc<ConfigurationStore>) -> io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let store = store.clone();
        tokio::spawn(async move {
            if let Err(error) = respond(stream, &store).await {
                eprintln!("backend request failed: {error}");
            }
        });
    }
}

async fn respond(mut stream: TcpStream, store: &ConfigurationStore) -> io::Result<()> {
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return write_response(
                &mut stream,
                "400 Bad Request",
                r#"{"error":"invalid_request"}"#,
                None,
            )
            .await;
        }
        Err(error) => return Err(error),
    };

    let response_origin = request
        .origin
        .as_deref()
        .filter(|origin| is_allowed_origin(origin))
        .map(str::to_owned);
    let (status, body) = route(request, store).await;
    write_response(&mut stream, status, &body, response_origin.as_deref()).await
}

fn is_allowed_origin(origin: &str) -> bool {
    matches!(
        origin,
        "http://tauri.localhost"
            | "tauri://localhost"
            | "http://127.0.0.1:1420"
            | "http://localhost:1420"
    )
}

async fn route(request: HttpRequest, store: &ConfigurationStore) -> (&'static str, String) {
    let (path, query) = request
        .path
        .split_once('?')
        .map_or((request.path.as_str(), None), |(path, query)| {
            (path, Some(query))
        });

    match (request.method.as_str(), path) {
        ("OPTIONS", "/configuration") => ("204 No Content", String::new()),
        ("GET", "/health") => ("200 OK", r#"{"state":"ready"}"#.to_owned()),
        ("GET", "/market-data/catalog") => market_data_catalog(query),
        ("GET", "/market-data/candles") => candle_history(query).await,
        ("GET", "/configuration") => match store.load() {
            Ok(configuration) => json_response(&configuration),
            Err(error) => {
                eprintln!("could not load user configuration: {error}");
                internal_error()
            }
        },
        ("PUT", "/configuration") => {
            let configuration = match serde_json::from_slice::<UserConfiguration>(&request.body) {
                Ok(configuration) => configuration,
                Err(_) => return bad_configuration(),
            };

            match store.save(&configuration) {
                Ok(configuration) => json_response(&configuration),
                Err(error) if error.is_invalid_configuration() => bad_configuration(),
                Err(error) => {
                    eprintln!("could not save user configuration: {error}");
                    internal_error()
                }
            }
        }
        _ => ("404 Not Found", r#"{"error":"not_found"}"#.to_owned()),
    }
}

fn market_data_catalog(query: Option<&str>) -> (&'static str, String) {
    let parameters = match parse_query(query.unwrap_or_default()) {
        Some(parameters) if parameters.len() == 1 => parameters,
        _ => return bad_market_data_request(),
    };
    let Some(provider_id) = parameters.get("provider") else {
        return bad_market_data_request();
    };

    let provider = match market_data_provider(provider_id) {
        Ok(provider) => provider,
        Err(_) => return market_data_not_found(),
    };

    match (provider.list_symbols(), provider.list_intervals()) {
        (Ok(instruments), Ok(intervals)) => json_response(&MarketDataCatalog {
            instruments,
            intervals,
        }),
        _ => internal_error(),
    }
}

async fn candle_history(query: Option<&str>) -> (&'static str, String) {
    let parameters = match parse_query(query.unwrap_or_default()) {
        Some(parameters) => parameters,
        None => return bad_market_data_request(),
    };
    let required = |name: &str| parameters.get(name).cloned();
    let request = match (
        required("provider"),
        required("symbol"),
        required("interval"),
    ) {
        (Some(provider), Some(symbol), Some(interval)) => CandleHistoryRequest {
            provider,
            symbol,
            interval,
            start_timestamp: match optional_i64(&parameters, "start_timestamp") {
                Ok(value) => value,
                Err(()) => return bad_market_data_request(),
            },
            end_timestamp: match optional_i64(&parameters, "end_timestamp") {
                Ok(value) => value,
                Err(()) => return bad_market_data_request(),
            },
            limit: match optional_u32(&parameters, "limit") {
                Ok(value) => value,
                Err(()) => return bad_market_data_request(),
            },
        },
        _ => return bad_market_data_request(),
    };

    let provider = match market_data_provider(&request.provider) {
        Ok(provider) => provider,
        Err(_) => return market_data_not_found(),
    };
    match provider.get_candles(&request).await {
        Ok(response) => json_response(&response),
        Err(
            ProviderError::UnknownProvider(_)
            | ProviderError::UnknownSymbol(_)
            | ProviderError::UnknownInterval(_),
        ) => market_data_not_found(),
        Err(ProviderError::InvalidRequest(_) | ProviderError::RequestLimitExceeded { .. }) => {
            bad_market_data_request()
        }
        Err(ProviderError::RateLimited { .. }) => (
            "429 Too Many Requests",
            r#"{"error":"market_data_rate_limited"}"#.to_owned(),
        ),
        Err(error) => {
            eprintln!("could not load candle history: {error}");
            internal_error()
        }
    }
}

fn market_data_provider(provider_id: &str) -> ProviderResult<Box<dyn MarketDataProvider>> {
    match provider_id {
        MOCK_PROVIDER_ID => Ok(Box::new(MockReplayProvider::sample(Duration::ZERO))),
        BINANCE_PROVIDER_ID => Ok(Box::new(BinanceSpotProvider::new()?)),
        _ => Err(ProviderError::UnknownProvider(provider_id.to_owned())),
    }
}

fn market_data_not_found() -> (&'static str, String) {
    (
        "404 Not Found",
        r#"{"error":"market_data_not_found"}"#.to_owned(),
    )
}

fn parse_query(query: &str) -> Option<HashMap<String, String>> {
    let mut parameters = HashMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=')?;
        if name.is_empty()
            || value.is_empty()
            || !matches!(
                name,
                "provider" | "symbol" | "interval" | "start_timestamp" | "end_timestamp" | "limit"
            )
            || parameters
                .insert(name.to_owned(), value.to_owned())
                .is_some()
        {
            return None;
        }
    }
    Some(parameters)
}

fn optional_i64(parameters: &HashMap<String, String>, name: &str) -> Result<Option<i64>, ()> {
    parameters
        .get(name)
        .map(|value| value.parse::<i64>().map_err(|_| ()))
        .transpose()
}

fn optional_u32(parameters: &HashMap<String, String>, name: &str) -> Result<Option<u32>, ()> {
    parameters
        .get(name)
        .map(|value| value.parse::<u32>().map_err(|_| ()))
        .transpose()
}

fn json_response(value: &impl Serialize) -> (&'static str, String) {
    match serde_json::to_string(value) {
        Ok(body) => ("200 OK", body),
        Err(error) => {
            eprintln!("could not encode backend response: {error}");
            internal_error()
        }
    }
}

fn bad_configuration() -> (&'static str, String) {
    (
        "400 Bad Request",
        r#"{"error":"invalid_configuration"}"#.to_owned(),
    )
}

fn bad_market_data_request() -> (&'static str, String) {
    (
        "400 Bad Request",
        r#"{"error":"invalid_market_data_request"}"#.to_owned(),
    )
}

fn internal_error() -> (&'static str, String) {
    (
        "500 Internal Server Error",
        r#"{"error":"internal_error"}"#.to_owned(),
    )
}

async fn read_request(stream: &mut TcpStream) -> io::Result<HttpRequest> {
    let mut bytes = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 1_024];
    let header_end = loop {
        let bytes_read = stream.read(&mut buffer).await?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before request headers",
            ));
        }
        bytes.extend_from_slice(&buffer[..bytes_read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request is too large",
            ));
        }
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };

    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "headers are not UTF-8"))?;
    let mut lines = headers.split("\r\n");
    let mut request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?
        .split_whitespace();
    let method = request_line
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request method"))?
        .to_owned();
    let path = request_line
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request path"))?
        .to_owned();
    let version = request_line
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing HTTP version"))?;
    if !version.starts_with("HTTP/1.") || request_line.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request line",
        ));
    }

    let mut content_length = 0;
    let mut origin = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid content length")
            })?;
        } else if name.eq_ignore_ascii_case("origin") {
            origin = Some(value.to_owned());
        }
    }
    if header_end + content_length > MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request is too large",
        ));
    }

    while bytes.len() < header_end + content_length {
        let bytes_read = stream.read(&mut buffer).await?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed before request body",
            ));
        }
        bytes.extend_from_slice(&buffer[..bytes_read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request is too large",
            ));
        }
    }

    Ok(HttpRequest {
        method,
        path,
        origin,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    body: &str,
    allowed_origin: Option<&str>,
) -> io::Result<()> {
    let access_control_headers = allowed_origin.map_or_else(String::new, |origin| {
        format!(
            "Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Methods: GET, PUT, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nVary: Origin\r\n"
        )
    });
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{access_control_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn request(listener: TcpListener, request: &str) -> String {
        let address = listener.local_addr().expect("listener address");
        let store = Arc::new(ConfigurationStore::open_in_memory().expect("open database"));
        let server = tokio::spawn(serve(listener, store));
        let mut stream = TcpStream::connect(address)
            .await
            .expect("connect to backend");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("send request");

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("read response");
        server.abort();
        response
    }

    #[tokio::test]
    async fn health_is_available_on_a_loopback_listener() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");
        assert!(listener
            .local_addr()
            .expect("listener address")
            .ip()
            .is_loopback());

        let response = request(listener, "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n").await;

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with(r#"{"state":"ready"}"#));
    }

    #[tokio::test]
    async fn configuration_can_be_loaded() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");

        let response = request(
            listener,
            "GET /configuration HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let (_, body) = response.split_once("\r\n\r\n").expect("HTTP response body");
        let configuration: UserConfiguration =
            serde_json::from_str(body).expect("configuration JSON");
        assert_eq!(configuration, UserConfiguration::default());
    }

    #[tokio::test]
    async fn deterministic_mock_candles_are_available_over_http() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");

        let response = request(
            listener,
            "GET /market-data/candles?provider=mock&symbol=BTCUSDT&interval=1h&limit=2 HTTP/1.1\r\nHost: localhost\r\nOrigin: http://tauri.localhost\r\n\r\n",
        )
        .await;
        let (_, body) = response.split_once("\r\n\r\n").expect("HTTP response body");
        let history: crate::market_data::CandleHistoryResponse =
            serde_json::from_str(body).expect("candle history JSON");

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Access-Control-Allow-Origin: http://tauri.localhost\r\n"));
        assert_eq!(history.candles.len(), 2);
        assert_eq!(history.candles[0].symbol, "BTCUSDT");
        assert!(history.candles[0].timestamp < history.candles[1].timestamp);
    }

    #[tokio::test]
    async fn provider_catalogs_are_available_over_http() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");

        let response = request(
            listener,
            "GET /market-data/catalog?provider=mock HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .await;
        let (_, body) = response.split_once("\r\n\r\n").expect("HTTP response body");
        let catalog: MarketDataCatalog =
            serde_json::from_str(body).expect("market-data catalog JSON");

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(catalog.instruments.len(), 5);
        assert_eq!(catalog.instruments[0].symbol, "BTCUSDT");
        assert_eq!(catalog.intervals.len(), 4);
        assert_eq!(catalog.intervals[2].id, "1h");

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");
        let response = request(
            listener,
            "GET /market-data/catalog?provider=binance HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .await;
        let (_, body) = response.split_once("\r\n\r\n").expect("HTTP response body");
        let catalog: MarketDataCatalog =
            serde_json::from_str(body).expect("market-data catalog JSON");

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(
            catalog
                .instruments
                .iter()
                .map(|instrument| instrument.symbol.as_str())
                .collect::<Vec<_>>(),
            ["BTCUSDT", "ETHUSDT", "BNBUSDT", "SOLUSDT", "XRPUSDT"]
        );
        assert_eq!(
            catalog
                .intervals
                .iter()
                .map(|interval| interval.id.as_str())
                .collect::<Vec<_>>(),
            [
                "1s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d",
                "3d", "1w", "1M"
            ]
        );
    }

    #[tokio::test]
    async fn malformed_market_data_queries_are_rejected() {
        let store = ConfigurationStore::open_in_memory().expect("open database");

        let (missing_status, _) = route(
            HttpRequest {
                method: "GET".to_owned(),
                path: "/market-data/candles?provider=mock&symbol=BTCUSDT".to_owned(),
                origin: None,
                body: Vec::new(),
            },
            &store,
        )
        .await;
        let (duplicate_status, _) = route(
            HttpRequest {
                method: "GET".to_owned(),
                path: "/market-data/candles?provider=mock&provider=mock&symbol=BTCUSDT&interval=1h"
                    .to_owned(),
                origin: None,
                body: Vec::new(),
            },
            &store,
        )
        .await;
        let (extra_catalog_status, _) = route(
            HttpRequest {
                method: "GET".to_owned(),
                path: "/market-data/catalog?provider=mock&symbol=BTCUSDT".to_owned(),
                origin: None,
                body: Vec::new(),
            },
            &store,
        )
        .await;

        assert_eq!(missing_status, "400 Bad Request");
        assert_eq!(duplicate_status, "400 Bad Request");
        assert_eq!(extra_catalog_status, "400 Bad Request");
    }

    #[tokio::test]
    async fn invalid_configuration_is_rejected() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");
        let body = r#"{"theme":"system","locale":"","time_zone":null,"workspace":{"tabs":[{"id":1,"provider":"binance","symbol":"BTCUSDT","interval":"1h","signals_visible":true,"viewport":{"visible_candle_count":null,"start_timestamp":null,"follow_latest":true}}],"active_tab_id":1}}"#;
        let raw_request = format!(
            "PUT /configuration HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );

        let response = request(listener, &raw_request).await;

        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.ends_with(r#"{"error":"invalid_configuration"}"#));
    }

    #[tokio::test]
    async fn configuration_updates_are_persisted_by_the_api() {
        let store = ConfigurationStore::open_in_memory().expect("open database");
        let body = br#"{"theme":"dark","locale":"en-GB","time_zone":"Europe/Kaliningrad","workspace":{"tabs":[{"id":1,"provider":"binance","symbol":"ETHUSDT","interval":"5m","signals_visible":false,"viewport":{"visible_candle_count":40,"start_timestamp":1704067200000,"follow_latest":false}}],"active_tab_id":1}}"#;

        let (save_status, _) = route(
            HttpRequest {
                method: "PUT".to_owned(),
                path: "/configuration".to_owned(),
                origin: None,
                body: body.to_vec(),
            },
            &store,
        )
        .await;
        let (load_status, saved) = route(
            HttpRequest {
                method: "GET".to_owned(),
                path: "/configuration".to_owned(),
                origin: None,
                body: Vec::new(),
            },
            &store,
        )
        .await;

        assert_eq!(save_status, "200 OK");
        assert_eq!(load_status, "200 OK");
        assert_eq!(saved.as_bytes(), body);
    }

    #[tokio::test]
    async fn configuration_writes_allow_the_desktop_origin_preflight() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");

        let response = request(
            listener,
            "OPTIONS /configuration HTTP/1.1\r\nHost: localhost\r\nOrigin: http://tauri.localhost\r\nAccess-Control-Request-Method: PUT\r\nAccess-Control-Request-Headers: content-type\r\n\r\n",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 204 No Content"));
        assert!(response.contains("Access-Control-Allow-Origin: http://tauri.localhost\r\n"));
        assert!(response.contains("Access-Control-Allow-Methods: GET, PUT, OPTIONS\r\n"));
        assert!(response.contains("Access-Control-Allow-Headers: Content-Type\r\n"));
    }

    #[tokio::test]
    async fn unknown_routes_are_not_exposed() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");

        let response = request(listener, "GET /unknown HTTP/1.1\r\nHost: localhost\r\n\r\n").await;

        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    }
}
