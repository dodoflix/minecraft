//! A local moderation service for the text filter: it records every request (method, target, headers, body) and
//! answers each from a script, so a vanilla and an `mc-server` run can be compared on what they sent and on how they
//! react to an answer.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::lock;

/// One request the mock received, in arrival order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterRequest {
    pub method: String,
    /// Path and query exactly as sent.
    pub target: String,
    /// Header names in lower case, in the order sent.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl FilterRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The request as one comparable text: method, target, the headers the filter sets itself, then the body.
    pub fn summary(&self) -> String {
        let headers: Vec<String> = ["content-type", "accept", "user-agent", "authorization"]
            .iter()
            .filter_map(|name| self.header(name).map(|v| format!("{name}: {v}")))
            .collect();
        format!(
            "{} {}\n{}\n{}",
            self.method,
            self.target,
            headers.join("\n"),
            self.body
        )
    }
}

/// How the mock answers a request.
#[derive(Clone, Debug)]
pub enum FilterAnswer {
    /// A status line and a JSON body.
    Json(u16, String),
    /// A status and no body.
    Status(u16),
    /// `302` to `location`.
    Redirect(String),
    /// Waits, then answers like `Json(200, body)`.
    Slow(Duration, String),
    /// Closes the connection without an answer.
    Close,
}

type Responder = Box<dyn Fn(&FilterRequest) -> FilterAnswer + Send + Sync>;

struct Shared {
    requests: Mutex<Vec<FilterRequest>>,
    responder: Mutex<Responder>,
    stop: AtomicBool,
}

/// A running mock. Dropping it stops the listener.
pub struct MockFilterServer {
    shared: Arc<Shared>,
    addr: SocketAddr,
}

impl MockFilterServer {
    /// Starts on a free loopback port; every request is answered with `{"response":true}` until
    /// [`Self::respond_with`].
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            requests: Mutex::new(Vec::new()),
            responder: Mutex::new(Box::new(|_| {
                FilterAnswer::Json(200, r#"{"response":true}"#.to_owned())
            })),
            stop: AtomicBool::new(false),
        });
        let served = shared.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if served.stop.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                let shared = served.clone();
                std::thread::spawn(move || {
                    let _ = serve(&shared, stream);
                });
            }
        });
        Ok(Self { shared, addr })
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Replaces how requests are answered.
    pub fn respond_with(
        &self,
        responder: impl Fn(&FilterRequest) -> FilterAnswer + Send + Sync + 'static,
    ) {
        *lock(&self.shared.responder) = Box::new(responder);
    }

    /// The requests so far.
    pub fn requests(&self) -> Vec<FilterRequest> {
        lock(&self.shared.requests).clone()
    }

    pub fn clear(&self) {
        lock(&self.shared.requests).clear();
    }
}

impl Drop for MockFilterServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
    }
}

fn serve(shared: &Shared, stream: TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
            return Ok(());
        };
        let (method, target) = (method.to_owned(), target.to_owned());
        let mut headers = Vec::new();
        loop {
            let mut header = String::new();
            reader.read_line(&mut header)?;
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
            }
        }
        let length = headers
            .iter()
            .find(|(k, _)| k == "content-length")
            .and_then(|(_, v)| v.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        let request = FilterRequest {
            method,
            target,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        };
        let answer = (lock(&shared.responder))(&request);
        lock(&shared.requests).push(request);
        match answer {
            FilterAnswer::Json(status, body) => write_response(&mut out, status, Some(&body), &[])?,
            FilterAnswer::Status(status) => write_response(&mut out, status, None, &[])?,
            FilterAnswer::Redirect(location) => {
                write_response(&mut out, 302, None, &[("Location", &location)])?
            }
            FilterAnswer::Slow(delay, body) => {
                std::thread::sleep(delay);
                write_response(&mut out, 200, Some(&body), &[])?
            }
            FilterAnswer::Close => return Ok(()),
        }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

fn write_response(
    out: &mut TcpStream,
    status: u16,
    body: Option<&str>,
    extra: &[(&str, &str)],
) -> io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    match body {
        Some(body) => {
            head.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ));
        }
        None => head.push_str("Content-Length: 0\r\n\r\n"),
    }
    out.write_all(head.as_bytes())?;
    out.flush()
}
