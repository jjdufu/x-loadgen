//! mazbri 网关官方客户端合同（不含 HTTP 栈，调用方自带 HTTP 客户端与计时）：
//!
//! - 换 token：`POST {base}/auth/token`，JSON `{"api_key", "grant_type": "client_credentials"}`；
//! - 调用：`POST {base}/api/v1/gateway`，`Authorization: Bearer <token>`，请求体为
//!   [`GatewayRequest`] JSON，或按租户 Profile 密封后的传输信封；
//! - 可选请求绑定：`x-mazbri-*` 五个头，签名覆盖最终发送的请求体；
//! - 响应：`mode=full` 时成功响应是信封，需解封后再按 [`GatewayResponseEnvelope`] 解析；
//! - 请求标识与链路追踪：`X-Request-Id`（`UUIDv7`）与 W3C `traceparent`，见 [`trace`]。
//!
//! [`TenantSettings`] 把页面录入的字符串字段收敛为 [`GatewaySession`]，
//! 校验规则与网关一致（闭集算法、legacy 门闩、suite 与显式算法不得冲突）。

mod contract;
mod session;
mod settings;
pub mod trace;

pub use contract::{
    GATEWAY_INVOKE_PATH, GRANT_TYPE_CLIENT_CREDENTIALS, GatewayRequest, GatewayResponseEnvelope,
    IssuedToken, TOKEN_PATH, TokenResponse, parse_token_response, token_request_body,
};
pub use session::{EncodedRequest, Error, GatewaySession, SessionConfig};
pub use settings::{
    EncryptionMode, TenantSettings, binding_signer, generate_binding_keypair, transport_profile,
};
pub use trace::{
    REQUEST_ID_HEADER, REQUEST_ID_MAX_LEN, TRACEPARENT_HEADER, TraceParent, is_valid_request_id,
    new_request_id,
};
pub use x_loadgen_core::crypto::request_binding::RequestSigner;
