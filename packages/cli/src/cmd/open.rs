use std::net::TcpListener;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as AsyncTcpListener;

use crate::history;

// Embed the site HTML at compile time
const SITE_HTML: &str = include_str!("../../site/index.html");

pub async fn run() -> Result<()> {
    // Find a free port
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.port()
    };

    let addr = format!("127.0.0.1:{port}");
    let listener = AsyncTcpListener::bind(&addr).await?;
    let url = format!("http://{addr}");

    println!("  Starting local server at {url}");
    println!("  Press Ctrl+C to stop\n");

    // Open browser
    open_browser(&url);

    // Serve requests
    loop {
        let (mut stream, _) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let n = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");

            let (status, content_type, body) = route(path);

            let response = format!(
                "HTTP/1.1 {status}\r\n\
                Content-Type: {content_type}\r\n\
                Content-Length: {}\r\n\
                Access-Control-Allow-Origin: *\r\n\
                Connection: close\r\n\
                \r\n",
                body.len()
            );

            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        });
    }
}

fn route(path: &str) -> (&'static str, &'static str, String) {
    match path {
        "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", SITE_HTML.to_string()),

        "/api/history" => {
            let json = match history::list_conversations() {
                Ok(list) => serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()),
                Err(_) => "[]".into(),
            };
            ("200 OK", "application/json; charset=utf-8", json)
        }

        p if p.starts_with("/api/history/") => {
            let id = p.trim_start_matches("/api/history/");
            match history::load_conversation(id) {
                Ok(conv) => {
                    let json =
                        serde_json::to_string(&conv).unwrap_or_else(|_| "{}".into());
                    ("200 OK", "application/json; charset=utf-8", json)
                }
                Err(_) => (
                    "404 Not Found",
                    "application/json",
                    r#"{"error":"not found"}"#.into(),
                ),
            }
        }

        _ => (
            "404 Not Found",
            "text/plain",
            "Not Found".into(),
        ),
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}
