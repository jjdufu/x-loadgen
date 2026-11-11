//! Simple request-body template substitution.
//!
//! Supported placeholders (case-sensitive):
//! - `{{uuid}}` — random UUID v4
//! - `{{seq}}` — 0-based request index
//! - `{{seq1}}` — 1-based request index
//! - `{{iso8601}}` — UTC timestamp `YYYY-MM-DDTHH:MM:SSZ`
//! - `{{random_int:MIN:MAX}}` — inclusive integer range
//! - `{{random_hex:N}}` — N random hex chars

use anyhow::{Result, bail};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Render placeholders in a request body template for `request_index`.
///
/// # Errors
///
/// Returns an error if a placeholder is malformed or cannot be substituted.
pub fn render_body_template(template: &str, request_index: u32) -> Result<String> {
    let mut out = String::with_capacity(template.len().saturating_add(32));
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{'
            && let Some(end) = find_close(bytes, i + 2)
        {
            let key = std::str::from_utf8(&bytes[i + 2..end])
                .map_err(|_| anyhow::anyhow!("template placeholder is not utf-8"))?
                .trim();
            out.push_str(&expand(key, request_index)?);
            i = end + 2;
            continue;
        }
        out.push(char::from(bytes[i]));
        i += 1;
    }
    Ok(out)
}

fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut j = from;
    while j + 1 < bytes.len() {
        if bytes[j] == b'}' && bytes[j + 1] == b'}' {
            return Some(j);
        }
        j += 1;
    }
    None
}

fn expand(key: &str, request_index: u32) -> Result<String> {
    match key {
        "uuid" => Ok(Uuid::new_v4().to_string()),
        "seq" => Ok(request_index.to_string()),
        "seq1" => Ok(request_index.saturating_add(1).to_string()),
        "iso8601" => Ok(format_iso8601_utc(unix_secs())?),
        _ if key.starts_with("random_int:") => expand_random_int(key),
        _ if key.starts_with("random_hex:") => expand_random_hex(key),
        _ => bail!("unknown body template placeholder: {{{{{key}}}}}"),
    }
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn expand_random_int(key: &str) -> Result<String> {
    let rest = key.strip_prefix("random_int:").unwrap_or(key);
    let mut parts = rest.split(':');
    let min: i64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("random_int needs MIN:MAX"))?
        .parse()?;
    let max: i64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("random_int needs MIN:MAX"))?
        .parse()?;
    if parts.next().is_some() {
        bail!("random_int expects exactly MIN:MAX");
    }
    if min > max {
        bail!("random_int MIN must be <= MAX");
    }
    let span = max.saturating_sub(min).saturating_add(1);
    let span_u = u64::try_from(span).unwrap_or(1);
    let pick = i64::try_from(mix64(unix_nanos()) % span_u.max(1)).unwrap_or(0);
    Ok((min.saturating_add(pick)).to_string())
}

fn expand_random_hex(key: &str) -> Result<String> {
    let n: usize = key
        .strip_prefix("random_hex:")
        .ok_or_else(|| anyhow::anyhow!("random_hex needs N"))?
        .parse()?;
    if n > 128 {
        bail!("random_hex N must be <= 128");
    }
    let mut state = mix64(unix_nanos());
    let mut out = String::with_capacity(n);
    for _ in 0..n {
        state = mix64(state);
        let nibble = usize::try_from(state & 0xf).unwrap_or(0);
        out.push(char::from_digit(u32::try_from(nibble).unwrap_or(0), 16).unwrap_or('0'));
    }
    Ok(out)
}

fn unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

fn mix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Format UTC civil time without pulling in a datetime crate.
fn format_iso8601_utc(secs: u64) -> Result<String> {
    let days = secs / 86_400;
    let tod = secs % 86_400;
    let (y, m, d) = civil_from_epoch_days(days)?;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    Ok(format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z"))
}

/// Howard Hinnant `civil_from_days`; `days` is days since 1970-01-01.
fn civil_from_epoch_days(days: u64) -> Result<(i32, u32, u32)> {
    let z = i64::try_from(days)
        .unwrap_or(i64::MAX)
        .saturating_add(719_468);
    let era = if z >= 0 { z } else { z.saturating_sub(146_096) } / 146_097;
    let doe = u64::try_from(z.saturating_sub(era.saturating_mul(146_097))).unwrap_or(0);
    let yoe = (doe
        .saturating_sub(doe / 1460)
        .saturating_add(doe / 36524)
        .saturating_sub(doe / 146_096))
        / 365;
    let y = i64::try_from(yoe)
        .unwrap_or(0)
        .saturating_add(era.saturating_mul(400));
    let doy = doe.saturating_sub(
        (365u64)
            .saturating_mul(yoe)
            .saturating_add(yoe / 4)
            .saturating_sub(yoe / 100),
    );
    let mp = (5u64.saturating_mul(doy).saturating_add(2)) / 153;
    let d = doy
        .saturating_sub((153u64.saturating_mul(mp).saturating_add(2)) / 5)
        .saturating_add(1);
    let m = if mp < 10 {
        mp.saturating_add(3)
    } else {
        mp.saturating_sub(9)
    };
    let y = if m <= 2 { y.saturating_add(1) } else { y };
    let year = i32::try_from(y).map_err(|_| anyhow::anyhow!("year out of range"))?;
    let month = u32::try_from(m).map_err(|_| anyhow::anyhow!("month out of range"))?;
    let day = u32::try_from(d).map_err(|_| anyhow::anyhow!("day out of range"))?;
    Ok((year, month, day))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_seq_and_uuid() {
        let out = render_body_template(r#"{"i":{{seq}},"id":"{{uuid}}"}"#, 3).unwrap();
        assert!(out.contains(r#""i":3"#));
        assert!(out.contains(r#""id":""#));
    }
}
