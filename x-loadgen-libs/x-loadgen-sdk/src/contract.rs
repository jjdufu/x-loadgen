//! 线上请求 / 响应结构与 token 交换。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::Error;

/// 网关调用路径（也是请求绑定签名覆盖的 `path_and_query`）。
pub const GATEWAY_INVOKE_PATH: &str = "/api/v1/gateway";
/// 换 token 路径。
pub const TOKEN_PATH: &str = "/auth/token";
/// 客户端凭据授权类型。
pub const GRANT_TYPE_CLIENT_CREDENTIALS: &str = "client_credentials";
/// `expires_in` 缺省值（秒）。
const DEFAULT_EXPIRES_IN_SECS: u64 = 1800;

/// 网关调用明文请求体（`mode=request|full` 时整体作为信封明文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayRequest {
    /// 业务产品 ID。
    pub product_id: String,
    /// 上游路径。
    pub path: String,
    /// 上游 HTTP 方法。
    pub method: String,
    /// 上游查询参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<BTreeMap<String, String>>,
    /// 透传给上游的请求头。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    /// 上游请求体。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

/// 网关响应（明文或解封后）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayResponseEnvelope {
    /// 业务是否成功。
    pub success: bool,
    /// 上游数据。
    #[serde(default)]
    pub data: Option<Value>,
    /// 错误说明（含限流原因等）。
    #[serde(default)]
    pub error: Option<String>,
    /// 网关业务状态码。
    pub status: u16,
    /// 网关请求 ID。
    #[serde(default)]
    pub request_id: Option<String>,
    /// 上游内容类型。
    #[serde(default)]
    pub content_type: Option<String>,
    /// 上游响应体编码。
    #[serde(default)]
    pub body_encoding: Option<String>,
}

/// `/auth/token` 响应。
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    /// 是否签发成功。
    pub success: bool,
    /// 访问令牌。
    #[serde(default)]
    pub access_token: Option<String>,
    /// 令牌类型（`Bearer`）。
    #[serde(default)]
    pub token_type: Option<String>,
    /// 有效期（秒）。
    #[serde(default)]
    pub expires_in: Option<u64>,
    /// 刷新令牌（客户端凭据模式下不使用）。
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// 租户信息。
    #[serde(default)]
    pub tenant_info: Option<Value>,
    /// 错误说明。
    #[serde(default)]
    pub error: Option<String>,
}

/// 已签发的访问令牌。
#[derive(Clone, PartialEq, Eq)]
pub struct IssuedToken {
    /// 访问令牌。
    pub access_token: String,
    /// 有效期（秒）。
    pub expires_in: u64,
}

impl std::fmt::Debug for IssuedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedToken")
            .field("access_token", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

/// `/auth/token` 请求体。
#[must_use]
pub fn token_request_body(api_key: &str) -> Value {
    serde_json::json!({
        "api_key": api_key,
        "grant_type": GRANT_TYPE_CLIENT_CREDENTIALS,
    })
}

/// 解析 `/auth/token` 响应。
///
/// # Errors
///
/// 响应不是合法 JSON、`success=false` 或缺少 `access_token` 时返回 [`Error::Token`]。
pub fn parse_token_response(http_status: u16, raw: &[u8]) -> Result<IssuedToken, Error> {
    let token: TokenResponse = serde_json::from_slice(raw).map_err(|error| {
        Error::Token(format!(
            "failed to parse token response (HTTP {http_status}): {error}"
        ))
    })?;
    if !token.success {
        return Err(Error::Token(token.error.unwrap_or_else(|| {
            format!("token request rejected with HTTP {http_status}")
        })));
    }
    let access_token = token
        .access_token
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Token("token response did not include access_token".into()))?;
    Ok(IssuedToken {
        access_token,
        expires_in: token.expires_in.unwrap_or(DEFAULT_EXPIRES_IN_SECS),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_body_uses_client_credentials() {
        let body = token_request_body("mazbri_abc_secret");
        assert_eq!(body["api_key"], "mazbri_abc_secret");
        assert_eq!(body["grant_type"], "client_credentials");
    }

    #[test]
    fn token_response_requires_success_and_access_token() {
        let ok = parse_token_response(
            200,
            br#"{"success":true,"access_token":"t1","token_type":"Bearer","expires_in":60}"#,
        )
        .expect("token");
        assert_eq!(ok.access_token, "t1");
        assert_eq!(ok.expires_in, 60);
        assert!(!format!("{ok:?}").contains("t1"));

        let defaulted =
            parse_token_response(200, br#"{"success":true,"access_token":"t2"}"#).expect("token");
        assert_eq!(defaulted.expires_in, DEFAULT_EXPIRES_IN_SECS);

        let rejected = parse_token_response(401, br#"{"success":false,"error":"bad api key"}"#);
        assert!(matches!(rejected, Err(Error::Token(msg)) if msg.contains("bad api key")));
        assert!(parse_token_response(200, br#"{"success":true,"access_token":" "}"#).is_err());
        assert!(parse_token_response(502, b"<html>").is_err());
    }

    #[test]
    fn gateway_request_omits_empty_optionals() {
        let request = GatewayRequest {
            product_id: "p1".into(),
            path: "/x".into(),
            method: "POST".into(),
            query: None,
            headers: None,
            body: Some(serde_json::json!({"k": 1})),
        };
        assert_eq!(
            serde_json::to_string(&request).expect("json"),
            r#"{"product_id":"p1","path":"/x","method":"POST","body":{"k":1}}"#
        );
    }
}
