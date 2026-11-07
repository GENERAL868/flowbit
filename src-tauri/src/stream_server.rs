//! The local HTTP server that streams torrent files to mpv while they download.
//!
//! It only streams: no torrent list, no other API. Websites open in a browser could
//! otherwise reach a server on localhost, so:
//! - URLs carry a random secret, so pages can't guess them to probe or embed files
//! - the Host header must be 127.0.0.1 or localhost, which defeats DNS rebinding
//! - only GET and HEAD, small request headers, and a time limit to send them

use std::{io::SeekFrom, net::Ipv4Addr, path::Path, sync::Arc, time::Duration};

use anyhow::Context;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::engine::Engine;

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct StreamServer {
    pub port: u16,
    pub secret: String,
}

impl StreamServer {
    /// Binds to `preferred_port` on 127.0.0.1, or any free port if it's taken
    pub async fn bind(preferred_port: u16, secret: String) -> anyhow::Result<(Self, TcpListener)> {
        let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, preferred_port)).await {
            Ok(l) => l,
            Err(_) => TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?,
        };
        let port = listener.local_addr()?.port();
        Ok((Self { port, secret }, listener))
    }

    pub fn url(&self, id: usize, file: usize, name: &str) -> String {
        format!(
            "http://127.0.0.1:{}/stream/{}/{}/{}/{}",
            self.port,
            self.secret,
            id,
            file,
            urlencode(name)
        )
    }
}

pub fn spawn(listener: TcpListener, server: Arc<StreamServer>, engine: std::sync::Weak<Engine>) {
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else { continue };
            let server = server.clone();
            let engine = engine.clone();
            tokio::spawn(async move {
                let _ = handle(socket, &server, engine).await;
            });
        }
    });
}

async fn handle(mut socket: TcpStream, server: &StreamServer, engine: std::sync::Weak<Engine>) -> anyhow::Result<()> {
    let request = match tokio::time::timeout(REQUEST_TIMEOUT, read_request(&mut socket)).await {
        Ok(Ok(r)) => r,
        _ => return respond(&mut socket, 400, "Bad Request", &[]).await,
    };

    // Only our own address, so a website can't reach us through DNS rebinding
    let host_ok = request.header("host").is_some_and(|h| {
        h == format!("127.0.0.1:{}", server.port) || h == format!("localhost:{}", server.port)
    });
    if !host_ok {
        return respond(&mut socket, 403, "Forbidden", &[]).await;
    }
    let head = match request.method.as_str() {
        "GET" => false,
        "HEAD" => true,
        _ => return respond(&mut socket, 405, "Method Not Allowed", &[("Allow", "GET, HEAD".into())]).await,
    };

    // /stream/<secret>/<torrent id>/<file index>[/<name>]
    let mut parts = request.path.trim_start_matches('/').split('/');
    let (Some("stream"), Some(secret), Some(id), Some(file)) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return respond(&mut socket, 404, "Not Found", &[]).await;
    };
    if !constant_time_eq(secret.as_bytes(), server.secret.as_bytes()) {
        return respond(&mut socket, 404, "Not Found", &[]).await;
    }
    let (Ok(id), Ok(file)) = (id.parse::<usize>(), file.parse::<usize>()) else {
        return respond(&mut socket, 404, "Not Found", &[]).await;
    };
    let Some(engine) = engine.upgrade() else {
        return respond(&mut socket, 503, "Service Unavailable", &[]).await;
    };
    let Ok(handle) = engine.handle(id) else {
        return respond(&mut socket, 404, "Not Found", &[]).await;
    };
    let name = engine
        .files(id)
        .ok()
        .and_then(|files| files.into_iter().find(|f| f.index == file))
        .map(|f| f.name)
        .unwrap_or_default();
    let Ok(mut stream) = handle.stream(file).await else {
        return respond(&mut socket, 404, "Not Found", &[]).await;
    };

    let len = stream.len();
    let content_type = mime_type(&name);
    let (status, start, end) = match request.header("range").map(|r| parse_range(r, len)) {
        None => (200, 0, len),
        Some(Some((start, end))) => (206, start, end),
        Some(None) => {
            return respond(&mut socket, 416, "Range Not Satisfiable", &[("Content-Range", format!("bytes */{len}"))]).await;
        }
    };

    let mut headers = vec![
        ("Content-Type", content_type.to_owned()),
        ("Content-Length", (end - start).to_string()),
        ("Accept-Ranges", "bytes".to_owned()),
    ];
    if status == 206 {
        headers.push(("Content-Range", format!("bytes {}-{}/{}", start, end.saturating_sub(1), len)));
    }
    let reason = if status == 206 { "Partial Content" } else { "OK" };
    write_head(&mut socket, status, reason, &headers).await?;
    if head || start == end {
        return Ok(());
    }
    stream.seek(SeekFrom::Start(start)).await?;
    let mut body = stream.take(end - start);
    tokio::io::copy(&mut body, &mut socket).await?;
    socket.shutdown().await.ok();
    Ok(())
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

async fn read_request(socket: &mut TcpStream) -> anyhow::Result<Request> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = socket.read(&mut chunk).await?;
        anyhow::ensure!(n > 0, "connection closed");
        buf.extend_from_slice(&chunk[..n]);
        anyhow::ensure!(buf.len() <= MAX_REQUEST_BYTES, "request too large");
    }
    let text = std::str::from_utf8(&buf).context("not utf-8")?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let method = first.next().unwrap_or_default().to_owned();
    let path = first.next().unwrap_or_default().to_owned();
    let headers = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Ok(Request { method, path, headers })
}

/// "bytes=a-b", "bytes=a-" or "bytes=-n" as [start, end)
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None; // several ranges: not needed by players
    }
    let (start, end) = spec.split_once('-')?;
    let (start, end) = if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        (len.saturating_sub(suffix), len)
    } else {
        let start: u64 = start.parse().ok()?;
        let end = if end.is_empty() { len } else { end.parse::<u64>().ok()?.saturating_add(1).min(len) };
        (start, end)
    };
    (start < end && start < len).then_some((start, end))
}

async fn write_head(socket: &mut TcpStream, status: u16, reason: &str, headers: &[(&str, String)]) -> anyhow::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    socket.write_all(head.as_bytes()).await?;
    Ok(())
}

async fn respond(socket: &mut TcpStream, status: u16, reason: &str, headers: &[(&str, String)]) -> anyhow::Result<()> {
    let mut all = headers.to_vec();
    all.push(("Content-Length", "0".to_owned()));
    write_head(socket, status, reason, &all).await?;
    socket.shutdown().await.ok();
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn mime_type(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "mkv" | "mka" => "video/x-matroska",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "ts" | "m2ts" | "mts" => "video/mp2t",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "opus" | "ogv" => "application/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 128 random bits from the system's secure random number generator, as hex
pub fn random_secret() -> String {
    let mut bytes = [0u8; 16];
    #[cfg(windows)]
    {
        #[link(name = "bcrypt")]
        extern "system" {
            fn BCryptGenRandom(alg: *mut std::ffi::c_void, buf: *mut u8, len: u32, flags: u32) -> i32;
        }
        const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 2;
        let status = unsafe {
            BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), bytes.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
        };
        assert!(status == 0, "BCryptGenRandom failed");
    }
    #[cfg(not(windows))]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .expect("no random source");
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 100)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 1000)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 1000)));
        assert_eq!(parse_range("bytes=990-2000", 1000), Some((990, 1000)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=5-1", 1000), None);
        assert_eq!(parse_range("bytes=0-1,5-6", 1000), None);
        assert_eq!(random_secret().len(), 32);
    }
}
