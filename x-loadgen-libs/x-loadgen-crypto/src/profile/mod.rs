//! 可版本化、可 serde 的租户 ↔ 网关传输 Profile。
//!
//! 不含厂商品牌字段；算法一律注册表 kebab-case id。

pub mod tenant_transport;

pub use tenant_transport::{
    BindingSpec, DEFAULT_CLOCK_SKEW_SECS, DEFAULT_NONCE_TTL_SECS, KeySelection, PROFILE_VERSION_V1,
    TenantTransportProfile, TransportMode,
};
