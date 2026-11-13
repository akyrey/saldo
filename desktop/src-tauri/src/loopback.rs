//! "Log in with Authentik" for the desktop app. The login runs in the
//! system browser, which then calls `http://127.0.0.1:<port>/callback?code=…`
//! on this app (RFC 8252's loopback redirect). `oidc_listen` opens that port,
//! waits for the one request in a background thread, answers the browser with
//! a short page, and passes the code (or an error) to the web app as the event
//! `oidc-callback`, like a Go handler pushing a value into a channel.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// The browser has this long to come back.
const WAIT: Duration = Duration::from_secs(300);
/// How often the non-blocking listener checks for the browser.
const POLL: Duration = Duration::from_millis(100);
/// The event the web app listens for.
pub const EVENT: &str = "oidc-callback";

/// What came back from the browser: a one-time code, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Answer {
    pub code: Option<String>,
    pub error: Option<String>,
}

impl Answer {
    fn error(code: &str) -> Self {
        Self {
            code: None,
            error: Some(code.to_string()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoopbackError {
    #[error("cannot open a local port for the login: {0}")]
    Io(#[from] std::io::Error),
}

impl Serialize for LoopbackError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Opens a random local port, returns it, and waits for the browser in the
/// background; the outcome arrives as the `oidc-callback` event.
#[tauri::command]
pub fn oidc_listen(app: AppHandle) -> Result<u16, LoopbackError> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    thread::spawn(move || {
        let answer = wait(&listener, WAIT);
        if let Some(window) = app.get_webview_window("main") {
            // Back to Saldo after the browser; a failure only costs a click.
            let _ = window.set_focus();
        }
        if let Err(error) = app.emit(EVENT, answer) {
            eprintln!("cannot deliver the login result: {error}");
        }
    });
    Ok(port)
}

/// Serves requests until `/callback` arrives or `limit` passes.
pub fn wait(listener: &TcpListener, limit: Duration) -> Answer {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(answer) = serve(stream) {
                    return answer;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(POLL),
            Err(_) => return Answer::error("oidc_unavailable"),
        }
    }
    Answer::error("oidc_expired")
}

/// Answers one browser request; `Some` when it was the callback.
fn serve(mut stream: TcpStream) -> Option<Answer> {
    // The accepted socket inherits non-blocking mode on some systems.
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).ok()?;
    let answer = parse_request_line(&line);
    let (status, body) = match &answer {
        Some(a) if a.code.is_some() => ("200 OK", PAGE_OK),
        Some(_) => ("200 OK", PAGE_FAILED),
        None => ("404 Not Found", ""),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    answer
}

const PAGE_OK: &str = "<!doctype html><meta charset=utf-8><title>Saldo</title>\
<body style=\"font-family:system-ui;text-align:center;padding:3rem\">\
<h1>You are logged in</h1><p>You can close this window and go back to Saldo.</p>";
const PAGE_FAILED: &str = "<!doctype html><meta charset=utf-8><title>Saldo</title>\
<body style=\"font-family:system-ui;text-align:center;padding:3rem\">\
<h1>The login did not work</h1><p>Go back to Saldo to see why and try again.</p>";

/// `GET /callback?code=…` → the code; `?error=…` → the error. Anything else
/// (a favicon request, another path) is not the callback.
pub fn parse_request_line(line: &str) -> Option<Answer> {
    let mut parts = line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return None;
    };
    let query = target.strip_prefix("/callback?")?;
    let value = |name: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.to_string())
            // Codes are base64url and errors are snake_case: nothing else is valid.
            .filter(|v| {
                !v.is_empty()
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
    };
    match (value("code"), value("error")) {
        (Some(code), None) => Some(Answer {
            code: Some(code),
            error: None,
        }),
        (_, Some(error)) => Some(Answer::error(&error)),
        (None, None) => Some(Answer::error("oidc_bad_request")),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::io::Read;

    use super::*;

    #[test]
    fn reads_the_code_or_the_error_from_the_callback() {
        let code = parse_request_line("GET /callback?code=abc-DEF_123 HTTP/1.1\r\n").unwrap();
        assert_eq!(code.code.as_deref(), Some("abc-DEF_123"));
        let error = parse_request_line("GET /callback?error=oidc_no_account HTTP/1.1").unwrap();
        assert_eq!(error.error.as_deref(), Some("oidc_no_account"));
        let odd = parse_request_line("GET /callback?code=<script> HTTP/1.1").unwrap();
        assert_eq!(odd.error.as_deref(), Some("oidc_bad_request"));
        assert!(parse_request_line("GET /favicon.ico HTTP/1.1").is_none());
        assert!(parse_request_line("POST /callback?code=x HTTP/1.1").is_none());
    }

    fn get(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn waits_past_other_requests_for_the_callback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let browser = thread::spawn(move || {
            let favicon = get(port, "/favicon.ico");
            let callback = get(port, "/callback?code=one-time");
            (favicon, callback)
        });

        let answer = wait(&listener, Duration::from_secs(5));

        assert_eq!(answer.code.as_deref(), Some("one-time"));
        let (favicon, callback) = browser.join().unwrap();
        assert!(favicon.starts_with("HTTP/1.1 404"));
        assert!(callback.contains("You are logged in"));
    }

    #[test]
    fn gives_up_when_the_browser_never_comes_back() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let answer = wait(&listener, Duration::from_millis(200));
        assert_eq!(answer.error.as_deref(), Some("oidc_expired"));
    }
}
