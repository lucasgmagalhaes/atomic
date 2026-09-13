//! RFC 1123 HTTP-date parsing (a small hand-rolled parser, no crate
//! dependency for one date format) — split out from `cookies.rs`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn month_index(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(name))
        .map(|i| i as u32)
}

/// Howard Hinnant's `days_from_civil`: days since the Unix epoch for a
/// given proleptic-Gregorian calendar date. Public-domain algorithm, no
/// leap-second/timezone handling needed since HTTP dates are always GMT.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parses an RFC 1123 HTTP-date: `"Wdy, DD Mon YYYY HH:MM:SS GMT"`.
pub(super) fn parse_http_date(s: &str) -> Option<SystemTime> {
    let parts: Vec<&str> = s.trim().split_whitespace().collect();
    if parts.len() != 6 {
        return None;
    }
    let day: u32 = parts[1].parse().ok()?;
    let month = month_index(parts[2])?;
    let year: i64 = parts[3].parse().ok()?;
    let mut time = parts[4].split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let min: i64 = time.next()?.parse().ok()?;
    let sec: i64 = time.next()?.parse().ok()?;

    let days = days_from_civil(year, month + 1, day);
    let epoch_secs = days * 86400 + hour * 3600 + min * 60 + sec;
    if epoch_secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(epoch_secs as u64))
}
