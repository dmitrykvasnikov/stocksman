use std::{
    io::{self, Write},
    net::Ipv4Addr,
    path::PathBuf,
    sync::Arc,
};

use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::database::{ConfigurationStore, UserConfiguration};

const MAX_REQUEST_BYTES: usize = 65_536;

#[derive(Serialize)]
struct BackendAnnouncement {
    port: u16,
}

struct HttpRequest {
    method: String,
    path: String,
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
            )
            .await;
        }
        Err(error) => return Err(error),
    };

    let (status, body) = route(request, store);
    write_response(&mut stream, status, &body).await
}

fn route(request: HttpRequest, store: &ConfigurationStore) -> (&'static str, String) {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => ("200 OK", r#"{"state":"ready"}"#.to_owned()),
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

    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse::<usize>())
        .transpose()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid content length"))?
        .unwrap_or(0);
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
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

async fn write_response(stream: &mut TcpStream, status: &str, body: &str) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
        assert!(response.ends_with(r#"{"theme":"system","locale":null,"time_zone":null}"#));
    }

    #[tokio::test]
    async fn invalid_configuration_is_rejected() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");
        let body = r#"{"theme":"system","locale":"","time_zone":null}"#;
        let raw_request = format!(
            "PUT /configuration HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );

        let response = request(listener, &raw_request).await;

        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.ends_with(r#"{"error":"invalid_configuration"}"#));
    }

    #[test]
    fn configuration_updates_are_persisted_by_the_api() {
        let store = ConfigurationStore::open_in_memory().expect("open database");
        let body = br#"{"theme":"dark","locale":"en-GB","time_zone":"Europe/Kaliningrad"}"#;

        let (save_status, _) = route(
            HttpRequest {
                method: "PUT".to_owned(),
                path: "/configuration".to_owned(),
                body: body.to_vec(),
            },
            &store,
        );
        let (load_status, saved) = route(
            HttpRequest {
                method: "GET".to_owned(),
                path: "/configuration".to_owned(),
                body: Vec::new(),
            },
            &store,
        );

        assert_eq!(save_status, "200 OK");
        assert_eq!(load_status, "200 OK");
        assert_eq!(saved.as_bytes(), body);
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
