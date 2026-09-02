use std::{
    io::{self, Write},
    net::Ipv4Addr,
};

use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const MAX_REQUEST_BYTES: usize = 4_096;

#[derive(Serialize)]
struct BackendAnnouncement {
    port: u16,
}

pub fn run() -> io::Result<()> {
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

            serve(listener).await
        })
}

async fn serve(listener: TcpListener) -> io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(error) = respond(stream).await {
                eprintln!("backend request failed: {error}");
            }
        });
    }
}

async fn respond(mut stream: TcpStream) -> io::Result<()> {
    let mut request = [0_u8; MAX_REQUEST_BYTES];
    let bytes_read = stream.read(&mut request).await?;
    let request = String::from_utf8_lossy(&request[..bytes_read]);

    let (status, body) = if request.starts_with("GET /health HTTP/1.") {
        ("200 OK", r#"{"state":"ready"}"#)
    } else {
        ("404 Not Found", r#"{"error":"not_found"}"#)
    };

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

    async fn request(listener: TcpListener, path: &str) -> String {
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(serve(listener));
        let mut stream = TcpStream::connect(address)
            .await
            .expect("connect to backend");
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
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

        let response = request(listener, "/health").await;

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with(r#"{"state":"ready"}"#));
    }

    #[tokio::test]
    async fn unknown_routes_are_not_exposed() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");

        let response = request(listener, "/unknown").await;

        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    }
}
