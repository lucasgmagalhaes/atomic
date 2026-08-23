//! Custom per-request DNS resolution: [`resolve_a`] is a hand-rolled
//! DNS-over-UDP client (RFC 1035 §4 wire format — the same "hand-roll the
//! one protocol piece actually needed, don't pull in a crate" convention
//! this workspace already applies to `storage::cookies`'s `Expires` date
//! parser and `css`'s tokenizer) that queries a caller-chosen DNS server
//! directly, bypassing the OS resolver entirely. [`get_via_dns`] uses it
//! to run one GET request against whatever `resolve_a` returns — the
//! network half of the mockup's "Settings: Network... DNS" gap (custom
//! DNS server, not just whatever the OS is configured with).
//!
//! Scoped down: A records (IPv4) only, no AAAA/CNAME-chasing beyond what
//! a single response packet already contains, no caching, no retry/
//! timeout beyond the OS socket default, UDP only (no TCP fallback for a
//! truncated response). One query per call, like every other `net`
//! function's "no connection pooling" scope.
use std::net::{Ipv4Addr, SocketAddr};

use tokio::net::{TcpStream, UdpSocket};

use crate::proxy::{send_one_request, wrap_tls};
use crate::{Error, Response};

/// Builds a minimal DNS query packet: a 12-byte header (random-ish
/// transaction ID, `RD` (recursion desired) set, one question) followed
/// by one QNAME/QTYPE=A/QCLASS=IN question.
fn build_query(id: u16, hostname: &str) -> Vec<u8> {
    let mut packet = Vec::with_capacity(hostname.len() + 32);
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&[0x01, 0x00]); // flags: RD=1, standard query
    packet.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
    packet.extend_from_slice(&[0x00, 0x00]); // ANCOUNT
    packet.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
    packet.extend_from_slice(&[0x00, 0x00]); // ARCOUNT

    for label in hostname.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0x00); // root label

    packet.extend_from_slice(&[0x00, 0x01]); // QTYPE = A
    packet.extend_from_slice(&[0x00, 0x01]); // QCLASS = IN
    packet
}

/// Skips one encoded name at `pos` (a sequence of length-prefixed labels
/// ending in a zero-length label, or a compression pointer per RFC 1035
/// §4.1.4 — `0xC0` high bits mean "the rest of the name lives at this
/// 14-bit offset instead"), returning the offset just past it.
fn skip_name(buf: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = *buf.get(pos)?;
        if len == 0 {
            return Some(pos + 1);
        }
        if len & 0xC0 == 0xC0 {
            // A pointer is always exactly 2 bytes; it never continues.
            return Some(pos + 2);
        }
        pos += 1 + len as usize;
    }
}

/// Parses a DNS response packet, returning the first A-record address
/// found in the answer section.
fn parse_a_record(buf: &[u8]) -> Option<Ipv4Addr> {
    if buf.len() < 12 {
        return None;
    }
    let qdcount = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let ancount = u16::from_be_bytes([buf[6], buf[7]]) as usize;

    let mut pos = 12;
    for _ in 0..qdcount {
        pos = skip_name(buf, pos)?;
        pos += 4; // QTYPE + QCLASS
    }

    for _ in 0..ancount {
        pos = skip_name(buf, pos)?;
        let rtype = u16::from_be_bytes([*buf.get(pos)?, *buf.get(pos + 1)?]);
        // TYPE(2) + CLASS(2) + TTL(4) + RDLENGTH(2) = 10 bytes of fixed
        // fields before RDATA.
        let rdlength = u16::from_be_bytes([*buf.get(pos + 8)?, *buf.get(pos + 9)?]) as usize;
        let rdata_start = pos + 10;
        if rtype == 1 && rdlength == 4 {
            let rdata = buf.get(rdata_start..rdata_start + 4)?;
            return Some(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]));
        }
        pos = rdata_start + rdlength;
    }
    None
}

/// Queries `dns_server` directly for `hostname`'s A record over UDP,
/// bypassing the OS resolver.
pub async fn resolve_a(hostname: &str, dns_server: SocketAddr) -> Result<Ipv4Addr, Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| Error::Request(format!("dns socket bind failed: {e}")))?;
    let id = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_millis()).unwrap_or(0)) as u16;
    let query = build_query(id, hostname);

    socket.send_to(&query, dns_server).await.map_err(|e| Error::Request(format!("dns query send failed: {e}")))?;

    let mut buf = [0u8; 512];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), socket.recv(&mut buf))
        .await
        .map_err(|_| Error::Request(format!("dns query to {dns_server} timed out")))?
        .map_err(|e| Error::Request(format!("dns response read failed: {e}")))?;

    parse_a_record(&buf[..n]).ok_or_else(|| Error::Request(format!("no A record found for {hostname} in DNS response from {dns_server}")))
}

/// Fetches `url` with a single GET request, resolving its host through
/// `dns_server` instead of the OS resolver — same calling convention as
/// [`crate::get_with_headers`]/[`crate::get_via_proxy`], just routed via
/// a caller-chosen DNS server. SNI/`Host` still use the URL's original
/// hostname (only the IP a real connection dials changes), matching how
/// a real browser's custom-DNS setting behaves.
pub fn get_via_dns(url: &str, extra_headers: &[(&str, &str)], dns_server: SocketAddr) -> Result<Response, Error> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| Error::Request(e.to_string()))?;
    runtime.block_on(get_via_dns_async(url, extra_headers, dns_server))
}

async fn get_via_dns_async(url: &str, extra_headers: &[(&str, &str)], dns_server: SocketAddr) -> Result<Response, Error> {
    let uri: hyper::Uri = url.parse().map_err(|e: hyper::http::uri::InvalidUri| Error::InvalidUrl(e.to_string()))?;
    let target_host = uri.host().ok_or_else(|| Error::InvalidUrl("missing host".to_string()))?.to_string();
    let is_https = uri.scheme_str() == Some("https");
    let target_port = uri.port_u16().unwrap_or(if is_https { 443 } else { 80 });

    let resolved_ip = resolve_a(&target_host, dns_server).await?;
    let stream = TcpStream::connect((resolved_ip, target_port))
        .await
        .map_err(|e| Error::Request(format!("connect to resolved address {resolved_ip}:{target_port} failed: {e}")))?;

    if is_https {
        let tls_stream = wrap_tls(stream, &target_host).await?;
        send_one_request(tls_stream, &uri, &target_host, extra_headers).await
    } else {
        send_one_request(stream, &uri, &target_host, extra_headers).await
    }
}

