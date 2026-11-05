/*
* Live search dashboard: a tiny HTTP server (std only) on the loopback interface.
*   GET /              the monitoring page (embedded in the binary)
*   GET /progress.json a snapshot of the shared search telemetry
* It never accepts input and only listens on 127.0.0.1.
*/

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::progress::Progress;

const PAGE: &str = include_str!("dashboard.html");

/// Starts the server on a background thread and returns the bound port (`port` 0 picks a free one).
pub fn serve(progress: Arc<Progress>, port: u16) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let bound = listener.local_addr()?.port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let progress = progress.clone();
            thread::spawn(move || {
                let _ = handle(stream, &progress);
            });
        }
    });
    Ok(bound)
}

fn handle(mut stream: TcpStream, progress: &Progress) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let mut parts = request.lines().next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/").split('?').next().unwrap_or("/");

    let (status, content_type, body) = if method != "GET" {
        ("405 Method Not Allowed", "text/plain; charset=utf-8", "method not allowed".to_string())
    } else {
        match path {
            "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", PAGE.to_string()),
            "/progress.json" => ("200 OK", "application/json", progress.to_json()),
            _ => ("404 Not Found", "text/plain; charset=utf-8", "not found".to_string()),
        }
    };
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    )?;
    stream.flush()
}

#[cfg(test)]
mod test {
    use super::*;

    fn request(port: u16, raw: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn serves_page_and_live_snapshot() {
        let progress = Progress::new();
        progress.begin("dashboard test", &[1, 2, 3]);
        progress.flush(&[1, 2, 0], 0);
        let port = serve(progress.clone(), 0).unwrap();

        let page = request(port, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(page.starts_with("HTTP/1.1 200 OK"));
        assert!(page.contains("<title>CAS3 search monitor</title>"));
        assert!(page.contains("/progress.json"));

        let first = request(port, "GET /progress.json?x=1 HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(first.contains("application/json"));
        assert!(first.contains("\"label\":\"dashboard test\""));
        assert!(first.contains("\"nodes\":3"));

        // The snapshot is live: later telemetry shows up on the next request.
        progress.flush(&[0, 0, 7], 1);
        let second = request(port, "GET /progress.json HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(second.contains("\"nodes\":10"));
        assert!(second.contains("\"solutions\":1"));

        assert!(request(port, "GET /missing HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 404"));
        assert!(request(port, "POST / HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 405"));
    }
}
