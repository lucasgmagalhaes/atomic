//! Tests for `fetch(url, { method, headers, body })` request options and
//! XHR method/body support — run against a real local HTTP server
//! (std-only `TcpListener`) that echoes what it received back as JSON.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;
use std::thread::sleep;
use std::time::Duration;

use js_runtime::{Context, Runtime};

/// Starts a tiny HTTP/1.1 echo server: responds 200 with a JSON body
/// describing the request (method, path, content-type header, raw body).
/// Returns the base URL plus a channel receiving each request summary.
fn start_echo_server() -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind localhost");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(match stream.try_clone() {
                Ok(s) => s,
                Err(_) => continue,
            });
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut content_length = 0usize;
            let mut content_type = String::new();
            let mut custom_headers = String::new();
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let lower = line.to_ascii_lowercase();
                        if let Some(v) = lower
                            .strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                        {
                            content_length = v;
                        }
                        if let Some(rest) = lower.strip_prefix("content-type:") {
                            content_type = rest.trim().to_string();
                        }
                        if lower.starts_with("x-custom:") || lower.starts_with("x-token:") {
                            custom_headers.push_str(lower.trim());
                            custom_headers.push(';');
                        }
                        if line == "\r\n" || line == "\n" {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let mut body = vec![0u8; content_length];
            if content_length > 0 && reader.read_exact(&mut body).is_err() {
                body.clear();
            }
            let summary = format!(
                "{}|{}|{}|{}|{}",
                request_line.split_whitespace().next().unwrap_or(""),
                request_line.split_whitespace().nth(1).unwrap_or(""),
                content_type,
                String::from_utf8_lossy(&body),
                custom_headers
            );
            let _ = tx.send(summary);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\necho",
                "echo".len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}"), rx)
}

fn pump_until<F: Fn(&Context) -> bool>(ctx: &Context, predicate: F, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        ctx.run_pending_timers();
        if predicate(ctx) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(10));
    }
}

#[test]
fn fetch_post_with_string_body_reaches_the_server() {
    let (base, requests) = start_echo_server();
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        &format!(
            r#"globalThis.seen = null;
               fetch("{base}/submit", {{
                 method: "POST",
                 headers: {{ "X-Custom": "yes" }},
                 body: "hello=world",
               }}).then((res) => {{ seen = res; }});"#
        ),
        "<test>",
    )
    .unwrap();

    let done = pump_until(
        &ctx,
        |c| c.eval("seen !== null", "<test>").unwrap() == "true",
        Duration::from_secs(5),
    );
    assert!(done, "fetch should resolve against the local server");
    assert_eq!(ctx.eval("seen.ok", "<test>").unwrap(), "true");
    assert_eq!(ctx.eval("seen.status", "<test>").unwrap(), "200");

    let summary = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let parts: Vec<&str> = summary.splitn(5, '|').collect();
    assert_eq!(parts[0], "POST", "method should be POST: {summary}");
    assert_eq!(parts[1], "/submit", "path: {summary}");
    assert!(
        parts[2].starts_with("text/plain"),
        "string bodies get the per-spec default content type: {summary}"
    );
    assert_eq!(parts[3], "hello=world", "body bytes: {summary}");
    assert!(
        parts[4].contains("x-custom: yes"),
        "custom header reached the server: {}",
        parts[4]
    );
}

#[test]
fn fetch_put_is_passed_through_and_default_is_get() {
    let (base, requests) = start_echo_server();
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        &format!("fetch(\"{base}/a\", {{ method: \"put\" }}).then(() => {{}});"),
        "<test>",
    )
    .unwrap();
    ctx.eval(&format!("fetch(\"{base}/b\").then(() => {{}});"), "<test>")
        .unwrap();

    let first = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    let second = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    // Order across two background threads isn't guaranteed; check the set.
    let summaries = [first, second];
    let methods: Vec<&str> = summaries
        .iter()
        .map(|s| s.split('|').next().unwrap_or(""))
        .collect();
    let mut sorted = methods.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec!["GET", "PUT"], "methods seen: {methods:?}");
}

#[test]
fn fetch_formdata_body_serializes_as_multipart() {
    let (base, requests) = start_echo_server();
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        &format!(
            r#"globalThis.seen = null;
               const fd = new FormData();
               fd.append("user", "alice");
               fd.append("doc", new File(["PDFDATA"], "f.pdf", {{ type: "application/pdf" }}));
               fetch("{base}/upload", {{ method: "POST", body: fd }}).then((res) => {{ seen = res; }});"#
        ),
        "<test>",
    )
    .unwrap();

    let done = pump_until(
        &ctx,
        |c| c.eval("seen !== null", "<test>").unwrap() == "true",
        Duration::from_secs(5),
    );
    assert!(done, "multipart upload should complete");

    let summary = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let parts: Vec<&str> = summary.splitn(5, '|').collect();
    assert_eq!(parts[0], "POST");
    assert!(
        parts[2].starts_with("multipart/form-data; boundary="),
        "auto Content-Type must carry the boundary: {}",
        parts[2]
    );
    assert!(
        parts[3].contains("alice"),
        "text field in body: {}",
        parts[3]
    );
    assert!(
        parts[3].contains("PDFDATA"),
        "file field bytes in body: {}",
        parts[3]
    );
    assert!(
        parts[3].contains("filename=\"f.pdf\""),
        "filename part: {}",
        parts[3]
    );
}

#[test]
fn xhr_send_honors_open_method_set_request_header_and_body() {
    let (base, requests) = start_echo_server();
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        &format!(
            r#"globalThis.done = false;
               const xhr = new XMLHttpRequest();
               xhr.open("post", "{base}/xhr");
               xhr.setRequestHeader("X-Token", "abc123");
               xhr.onload = () => {{ done = true; }};
               xhr.send("payload=1");"#
        ),
        "<test>",
    )
    .unwrap();

    let done = pump_until(
        &ctx,
        |c| c.eval("done", "<test>").unwrap() == "true",
        Duration::from_secs(5),
    );
    assert!(done, "XHR onload should fire");

    let summary = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let parts: Vec<&str> = summary.splitn(5, '|').collect();
    assert_eq!(parts[0], "POST", "open() method honored: {summary}");
    assert_eq!(parts[3], "payload=1", "send(body) forwarded: {summary}");
    assert!(
        parts[4].contains("x-token: abc123"),
        "setRequestHeader reached the server: {}",
        parts[4]
    );
}
