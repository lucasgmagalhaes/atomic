//! HTTP/TLS client — the spec's one deliberate exception to "no
//! third-party engine dependencies": `hyper` + `rustls`, since rolling a
//! custom TLS stack is a security risk this project isn't taking on.
//! Exposes a synchronous `get()` (internally runs its own single-use
//! `tokio` runtime, same "sync facade over async work" pattern used
//! elsewhere in this workspace for `pollster`-driven `wgpu` calls) rather
//! than forcing every caller onto async — none of `dom`/`layout-engine`/
//! `js-runtime` are async, and this crate doesn't need to be either yet.
//!
//! Scoped to a single GET request with no redirect following, no cookie
//! jar, no caching, no connection pooling across calls (a fresh runtime
//! + client per call), no proxy support yet (the spec's `net` crate row
//! calls for per-profile proxy — not implemented here). `rustls`'s `ring`
//! crypto backend, not `aws-lc-rs` (the crate default) — `aws-lc-rs`
//! needs `cmake`/`nasm` to build its C code, which this dev machine
//! doesn't have set up; `ring` is pure Rust (well, Rust + a
//! pre-vendored C core built without extra tooling) and just works.
use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum Error {
    InvalidUrl(String),
    Tls(String),
    Request(String),
    Body(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidUrl(e) => write!(f, "invalid URL: {e}"),
            Error::Tls(e) => write!(f, "TLS setup failed: {e}"),
            Error::Request(e) => write!(f, "request failed: {e}"),
            Error::Body(e) => write!(f, "failed to read response body: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Fetches `url` with a single GET request, blocking the calling thread
/// until the response body is fully read.
pub fn get(url: &str) -> Result<Response, Error> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| Error::Request(e.to_string()))?;
    runtime.block_on(get_async(url))
}

async fn get_async(url: &str) -> Result<Response, Error> {
    let uri: hyper::Uri = url.parse().map_err(|e: hyper::http::uri::InvalidUri| Error::InvalidUrl(e.to_string()))?;

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|e| Error::Tls(e.to_string()))?
        .https_or_http()
        .enable_http1()
        .build();
    let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new()).build(https);

    let res = client.get(uri).await.map_err(|e| Error::Request(e.to_string()))?;
    let status = res.status().as_u16();
    let body = res
        .into_body()
        .collect()
        .await
        .map_err(|e| Error::Body(e.to_string()))?
        .to_bytes()
        .to_vec();

    Ok(Response { status, body })
}
