//! 请求标识与链路追踪（mazbri 对接协议「请求标识与链路追踪」）。
//!
//! - `X-Request-Id`：调用方流水号，字符集 `[A-Za-z0-9._-]`、长度 1..=128。网关原样透传给上游，
//!   不合规时由网关换成新 UUID。客户端用 [`new_request_id`] 生成 `UUIDv7`（小写带连字符）。
//! - `traceparent`：W3C Trace Context `00-<32 位 hex trace-id>-<16 位 hex parent-id>-<2 位 hex flags>`。
//!   网关把入站 `traceparent` 接到同一条调用链，响应头回写网关 span 的 `traceparent`，
//!   trace-id 不变；flags 的最低位 `01` 表示采样。

use std::fmt;

use uuid::Uuid;

/// 流水号请求 / 响应头名。
pub const REQUEST_ID_HEADER: &str = "x-request-id";
/// W3C Trace Context 请求 / 响应头名。
pub const TRACEPARENT_HEADER: &str = "traceparent";
/// 流水号最大长度。
pub const REQUEST_ID_MAX_LEN: usize = 128;

/// 流水号是否符合协议字符集与长度（`[A-Za-z0-9._-]{1,128}`）。
#[must_use]
pub fn is_valid_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= REQUEST_ID_MAX_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// 新流水号：`UUIDv7`（按时间有序，小写带连字符）。
#[must_use]
pub fn new_request_id() -> String {
    Uuid::now_v7().to_string()
}

/// W3C `traceparent`（版本 `00`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceParent {
    trace_id: [u8; 16],
    parent_id: [u8; 8],
    sampled: bool,
}

impl TraceParent {
    /// 随机生成 trace-id 与 parent-id（均非全零），`sampled` 决定 flags 是 `01` 还是 `00`。
    #[must_use]
    pub fn generate(sampled: bool) -> Self {
        let mut trace_id = random_bytes::<16>();
        let mut parent_id = random_bytes::<8>();
        if trace_id == [0; 16] {
            trace_id[15] = 1;
        }
        if parent_id == [0; 8] {
            parent_id[7] = 1;
        }
        Self {
            trace_id,
            parent_id,
            sampled,
        }
    }

    /// 按 W3C 规则解析：版本 2 位 hex 且不是 `ff`；trace-id / parent-id 为小写 hex 且非全零；
    /// 版本 `00` 必须正好 4 段。更高版本只取前 4 段。
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let mut parts = value.split('-');
        let version = parts.next()?;
        let trace = parts.next()?;
        let parent = parts.next()?;
        let flags = parts.next()?;
        if version.len() != 2 || !is_lower_hex(version) || version == "ff" {
            return None;
        }
        if version == "00" && parts.next().is_some() {
            return None;
        }
        if flags.len() != 2 || !is_lower_hex(flags) {
            return None;
        }
        let trace_id: [u8; 16] = decode_hex(trace)?;
        let parent_id: [u8; 8] = decode_hex(parent)?;
        if trace_id == [0; 16] || parent_id == [0; 8] {
            return None;
        }
        let flag_bits = u8::from_str_radix(flags, 16).ok()?;
        Some(Self {
            trace_id,
            parent_id,
            sampled: flag_bits & 0x01 == 0x01,
        })
    }

    /// 32 位小写 hex trace-id。
    #[must_use]
    pub fn trace_id(&self) -> String {
        encode_hex(&self.trace_id)
    }

    /// 16 位小写 hex parent-id（本跳 span id）。
    #[must_use]
    pub fn parent_id(&self) -> String {
        encode_hex(&self.parent_id)
    }

    /// flags 是否带采样位。
    #[must_use]
    pub const fn sampled(&self) -> bool {
        self.sampled
    }

    /// 两个 `traceparent` 是否属于同一条调用链（trace-id 相同）。
    #[must_use]
    pub fn same_trace(&self, other: &Self) -> bool {
        self.trace_id == other.trace_id
    }

    /// 以新的 parent-id 延续同一条调用链（网关、mock 回写响应头时使用）。
    #[must_use]
    pub fn child(&self) -> Self {
        let fresh = Self::generate(self.sampled);
        Self {
            trace_id: self.trace_id,
            parent_id: fresh.parent_id,
            sampled: self.sampled,
        }
    }
}

impl fmt::Display for TraceParent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "00-{}-{}-{}",
            self.trace_id(),
            self.parent_id(),
            if self.sampled { "01" } else { "00" }
        )
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0_u8; N];
    if getrandom::fill(&mut out).is_err() {
        // 系统随机源不可用时退回 UUIDv4 的随机位，仍满足「非全零、足够随机」。
        let mut filled = 0;
        while filled < N {
            let chunk = Uuid::new_v4().into_bytes();
            let take = chunk.len().min(N - filled);
            out[filled..filled + take].copy_from_slice(&chunk[..take]);
            filled += take;
        }
    }
    out
}

fn is_lower_hex(text: &str) -> bool {
    text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn decode_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !is_lower_hex(text) {
        return None;
    }
    let mut out = [0_u8; N];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

fn encode_hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_is_uuid_v7_and_protocol_valid() {
        let id = new_request_id();
        let parsed = Uuid::parse_str(&id).expect("uuid");
        assert_eq!(parsed.get_version_num(), 7);
        assert_eq!(id, id.to_ascii_lowercase());
        assert!(is_valid_request_id(&id));
        assert!(is_valid_request_id(&format!("probe-{id}")));
        assert!(!is_valid_request_id(""));
        assert!(!is_valid_request_id("has space"));
        assert!(!is_valid_request_id("中文"));
        assert!(!is_valid_request_id(&"a".repeat(129)));
        assert!(is_valid_request_id(&"a".repeat(128)));
    }

    #[test]
    fn generated_traceparent_roundtrips_with_flags() {
        let sampled = TraceParent::generate(true);
        let text = sampled.to_string();
        assert_eq!(text.len(), 55);
        assert!(text.starts_with("00-") && text.ends_with("-01"));
        assert_eq!(TraceParent::parse(&text), Some(sampled));
        let unsampled = TraceParent::generate(false);
        assert!(unsampled.to_string().ends_with("-00"));
        assert!(
            !TraceParent::parse(&unsampled.to_string())
                .expect("parse")
                .sampled()
        );
        assert_ne!(sampled.trace_id(), unsampled.trace_id());
    }

    #[test]
    fn parse_follows_w3c_rules() {
        let ok = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let tp = TraceParent::parse(ok).expect("valid");
        assert_eq!(tp.trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(tp.parent_id(), "00f067aa0ba902b7");
        assert!(tp.sampled());
        for bad in [
            "",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
            "00-4bf92f3577b34da6a3ce929d0e0e473-00f067aa0ba902b7-01",
        ] {
            assert!(TraceParent::parse(bad).is_none(), "{bad}");
        }
        let future = "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00-ext";
        assert!(TraceParent::parse(future).is_some());
    }

    #[test]
    fn child_keeps_trace_and_flags_with_new_parent() {
        let root = TraceParent::generate(false);
        let child = root.child();
        assert!(root.same_trace(&child));
        assert_ne!(root.parent_id(), child.parent_id());
        assert!(!child.sampled());
    }
}
