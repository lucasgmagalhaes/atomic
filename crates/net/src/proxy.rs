//! Per-request HTTP/HTTPS proxying: [`ProxyConfig`] describes one upstream
//! proxy (host, port, optional Basic auth), and [`get_via_proxy`] tunnels
//! through it via a real `CONNECT` request (RFC 7231 §4.3.6) before running
//! one HTTP/1.1 request over the tunnel. Works for both `http://` and
//! `https://` targets — `CONNECT` establishes a raw byte tunnel to
//! `target_host:target_port` regardless of the target's own scheme, so an
//! `https://` target gets a real TLS handshake *through* the tunnel (SNI
//! and the certificate chain are checked against the target host, not the
//! proxy — the proxy never terminates TLS, matching how a real forward
//! proxy is meant to work) while an `http://` target just runs its request
//! straight over the tunnel's plaintext bytes.
//!
//! Scoped the same way [`crate::get`]/[`crate::get_with_headers`] are: one
//! request per call (`hyper::client::conn::http1::handshake` directly, not
//! `hyper_util`'s pooling `Client`), no connection reuse across calls, no
//! proxy auth beyond HTTP Basic, no `CONNECT`-retry/redirect handling.
use std::sync::Arc;

use base64::Engine;
use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::header::{HeaderName, HeaderValue};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::{Error, Response};

/// One upstream HTTP/HTTPS proxy a request can be tunneled through.
/// `username`/`password` both `None` sends no `Proxy-Authorization` header
/// at all (an anonymous proxy); either one present sends HTTP Basic auth
/// with the other defaulting to an empty string.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyConfig {
    fn proxy_authorization(&self) -> Option<String> {
        if self.username.is_none() && self.password.is_none() {
            return None;
        }
        let user = self.username.as_deref().unwrap_or("");
        let pass = self.password.as_deref().unwrap_or("");
        let creds = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        Some(format!("Basic {creds}"))
    }
}

/// Fetches `url` through `proxy` with a single GET request, blocking the
/// calling thread until the response body is fully read — same calling
/// convention as [`crate::get_with_headers`], just routed through a
/// `CONNECT` tunnel instead of a direct connection.
pub fn get_via_proxy(url: &str, extra_headers: &[(&str, &str)], proxy: &ProxyConfig) -> Result<Response, Error> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| Error::Request(e.to_string()))?;
    runtime.block_on(get_via_proxy_async(url, extra_headers, proxy))
}

async fn get_via_proxy_async(url: &str, extra_headers: &[(&str, &str)], proxy: &ProxyConfig) -> Result<Response, Error> {
    let uri: hyper::Uri = url.parse().map_err(|e: hyper::http::uri::InvalidUri| Error::InvalidUrl(e.to_string()))?;
    let target_host = uri.host().ok_or_else(|| Error::InvalidUrl("missing host".to_string()))?.to_string();
    let is_https = uri.scheme_str() == Some("https");
    let target_port = uri.port_u16().unwrap_or(if is_https { 443 } else { 80 });

    let tunnel = open_connect_tunnel(proxy, &target_host, target_port).await?;

    if is_https {
        let tls_stream = wrap_tls(tunnel, &target_host).await?;
        send_one_request(tls_stream, &uri, &target_host, extra_headers).await
    } else {
        send_one_request(tunnel, &uri, &target_host, extra_headers).await
    }
}

/// Connects to `proxy`, sends a `CONNECT target_host:target_port` request,
/// and returns the raw `TcpStream` positioned right after the proxy's
/// blank-line-terminated response — everything from that point on belongs
/// to whatever protocol the caller tunnels next (TLS handshake bytes for
/// an `https://` target, a plain HTTP/1.1 request otherwise), not to this
/// handshake.
async fn open_connect_tunnel(proxy: &ProxyConfig, target_host: &str, target_port: u16) -> Result<TcpStream, Error> {
    let mut stream = TcpStream::connect((proxy.host.as_str(), proxy.port))
        .await
        .map_err(|e| Error::Request(format!("proxy connect to {}:{} failed: {e}", proxy.host, proxy.port)))?;

    let mut connect_request = format!("CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n");
    if let Some(auth) = proxy.proxy_authorization() {
        connect_request.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
    }
    connect_request.push_str("\r\n");
    stream
        .write_all(connect_request.as_bytes())
        .await
        .map_err(|e| Error::Request(format!("proxy CONNECT write failed: {e}")))?;

    // Read the CONNECT response status line + headers ourselves (a plain
    // buffered line reader, not a full HTTP client) — we only need the
    // status code before treating the rest of the connection as a raw
    // tunnel, and `BufReader::into_inner` hands back the exact same
    // `TcpStream` with its read cursor left right after the blank line,
    // discarding nothing.
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .await
        .map_err(|e| Error::Request(format!("proxy CONNECT response read failed: {e}")))?;
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::Request(format!("malformed proxy CONNECT response: {}", status_line.trim())))?;

    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|e| Error::Request(format!("proxy CONNECT response read failed: {e}")))?;
        if n == 0 || line == "\r\n" {
            break;
        }
    }

    if status_code != 200 {
        return Err(Error::Request(format!("proxy CONNECT to {target_host}:{target_port} failed with status {status_code}")));
    }

    Ok(reader.into_inner())
}

pub(crate) async fn wrap_tls(stream: TcpStream, target_host: &str) -> Result<tokio_rustls::client::TlsStream<TcpStream>, Error> {
    let mut root_store = rustls::RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    for cert in loaded.certs {
        let _ = root_store.add(cert);
    }
    let config = rustls::ClientConfig::builder().with_root_certificates(root_store).with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let server_name = rustls::pki_types::ServerName::try_from(target_host.to_string()).map_err(|e| Error::Tls(e.to_string()))?;
    connector.connect(server_name, stream).await.map_err(|e| Error::Tls(e.to_string()))
}

/// Runs one GET request over an already-established stream (a raw
/// `TcpStream` tunnel for an `http://` target, a `TlsStream` wrapping one
/// for `https://`) via `hyper`'s low-level `client::conn::http1` — the
/// tunnel is only good for one connection, so there's no `hyper_util`
/// pooling `Client` to hand it to.
pub(crate) async fn send_one_request<S>(stream: S, uri: &hyper::Uri, target_host: &str, extra_headers: &[(&str, &str)]) -> Result<Response, Error>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(stream);
    let (mut sender, connection) = hyper::client::conn::http1::handshake(io).await.map_err(|e| Error::Request(e.to_string()))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let path_and_query = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let mut request = hyper::Request::get(path_and_query)
        .header("Host", target_host)
        .body(Empty::<Bytes>::new())
        .map_err(|e| Error::Request(e.to_string()))?;
    for (name, value) in extra_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|e| Error::Request(e.to_string()))?;
        let value = HeaderValue::from_str(value).map_err(|e| Error::Request(e.to_string()))?;
        request.headers_mut().insert(name, value);
    }

    let res = sender.send_request(request).await.map_err(|e| Error::Request(e.to_string()))?;
    let status = res.status().as_u16();
    let headers = res
        .headers()
        .iter()
        .map(|(name, value)| (name.as_str().to_string(), value.to_str().unwrap_or("").to_string()))
        .collect();
    let body = res
        .into_body()
        .collect()
        .await
        .map_err(|e| Error::Body(e.to_string()))?
        .to_bytes()
        .to_vec();

    Ok(Response { status, body, headers })
}
