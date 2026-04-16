use std::net::TcpListener;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as AsyncTcpListener;

use crate::history;
use crate::log as cblog;

const SITE_HTML: &str = include_str!("../../site/index.html");
const DASHBOARD_HTML: &str = include_str!("../../site/dashboard.html");

pub async fn run() -> Result<()> {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.port()
    };

    let addr = format!("127.0.0.1:{port}");
    let listener = AsyncTcpListener::bind(&addr).await?;
    let base_url = format!("http://{addr}");
    let dashboard_url = format!("{base_url}/dashboard");

    println!("  \x1b[96m●\x1b[0m  Dashboard → \x1b[1m{dashboard_url}\x1b[0m");
    println!("  \x1b[90m   Press Ctrl+C to stop\x1b[0m");
    println!();

    open_browser(&dashboard_url);

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

            // Strip query string for routing
            let path_only = path.split('?').next().unwrap_or(path);
            let query = path.split('?').nth(1).unwrap_or("");

            let (status, content_type, body) = route(path_only, query);

            let response = format!(
                "HTTP/1.1 {status}\r\n\
                Content-Type: {content_type}\r\n\
                Content-Length: {}\r\n\
                Access-Control-Allow-Origin: *\r\n\
                Cache-Control: no-cache\r\n\
                Connection: close\r\n\
                \r\n",
                body.len()
            );

            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        });
    }
}

fn route(path: &str, query: &str) -> (&'static str, &'static str, String) {
    match path {
        "/" | "/index.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            SITE_HTML.to_string(),
        ),

        "/dashboard" => (
            "200 OK",
            "text/html; charset=utf-8",
            DASHBOARD_HTML.to_string(),
        ),

        // ── History API (existing) ────────────────────────────────────────────
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
                    let json = serde_json::to_string(&conv).unwrap_or_else(|_| "{}".into());
                    ("200 OK", "application/json; charset=utf-8", json)
                }
                Err(_) => (
                    "404 Not Found",
                    "application/json",
                    r#"{"error":"not found"}"#.into(),
                ),
            }
        }

        // ── Events API (new) ──────────────────────────────────────────────────

        // GET /api/events          → today's events
        // GET /api/events?date=YYYY-MM-DD → specific date
        "/api/events" => {
            let events = if let Some(date) = parse_query_param(query, "date") {
                cblog::read_date(&date)
            } else {
                cblog::today_events()
            };
            let json = serde_json::to_string(&events).unwrap_or_else(|_| "[]".into());
            ("200 OK", "application/json; charset=utf-8", json)
        }

        // GET /api/events/dates → list of available dates, newest first
        "/api/events/dates" => {
            let dates = cblog::list_dates();
            let json = serde_json::to_string(&dates).unwrap_or_else(|_| "[]".into());
            ("200 OK", "application/json; charset=utf-8", json)
        }

        _ => ("404 Not Found", "text/plain", "Not Found".into()),
    }
}

fn parse_query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next()?;
        let v = parts.next()?;
        if k == key {
            Some(v.to_string())
        } else {
            None
        }
    })
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
