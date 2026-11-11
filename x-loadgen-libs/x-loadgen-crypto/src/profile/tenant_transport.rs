//! 租户 ↔ 网关传输 Profile。

use serde::{Deserialize, Serialize};

use crate::id::{AlgorithmId, AlgorithmKind};
use crate::legacy::assert_not_legacy_or_allowed;
use crate::registry::{AlgorithmRegistry, require_mac_for};
use crate::suite::{SuiteSpec, resolve_suite};
use crate::traits::CryptoError;

/// Profile schema 版本（本步起点）。
pub const PROFILE_VERSION_V1: u32 = 1;

/// 默认 nonce TTL（秒），与现网 binding 默认对齐。
pub const DEFAULT_NONCE_TTL_SECS: u32 = 300;

/// 默认时钟偏移容忍（秒）。
pub const DEFAULT_CLOCK_SKEW_SECS: u32 = 300;

/// 租户传输模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportMode {
    /// 不加密（亦不要求传输层签名）。
    None,
    /// 仅请求侧加密 / AEAD。
    Request,
    /// 请求 + 响应双向同一套件。
    Full,
}

/// 请求绑定策略（nonce / 时间窗等细节由网关侧执行；此处仅开关与标记）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingSpec {
    /// 是否强制请求绑定。
    pub required: bool,
}

/// 密钥选择策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum KeySelection {
    /// 固定 key id + 版本。
    Fixed {
        /// 密钥标识。
        key_id: String,
        /// 密钥版本。
        key_version: u32,
    },
    /// 使用该 key id 的当前激活版本。
    Latest {
        /// 密钥标识。
        key_id: String,
    },
}

impl KeySelection {
    /// 取出 key id。
    #[must_use]
    pub fn key_id(&self) -> &str {
        match self {
            Self::Fixed { key_id, .. } | Self::Latest { key_id } => key_id,
        }
    }
}

/// 租户传输 Profile（可版本化 / serde；后续 DB / `OpenAPI` 对齐）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantTransportProfile {
    /// Schema 版本。
    pub version: u32,
    /// 传输模式。
    pub mode: TransportMode,
    /// 命名套件（可选）；与 `data_alg` / `mac_alg` 同时出现时必须一致。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    /// 数据算法（`mode != none` 时必填或由 suite 解析）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_alg: Option<AlgorithmId>,
    /// 外挂 MAC（CBC/CTR/CFB 等非 AEAD **必须**）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_alg: Option<AlgorithmId>,
    /// 密钥选择；加密模式下必填。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<KeySelection>,
    /// 是否要求外层签名 / MAC（与 AEAD 完整性可叠加）。
    #[serde(default)]
    pub require_signature: bool,
    /// 是否允许 Legacy 算法。
    #[serde(default)]
    pub allow_insecure: bool,
    /// 可选请求绑定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<BindingSpec>,
    /// Nonce 存活时间（秒）。
    pub nonce_ttl_secs: u32,
    /// 时钟偏移容忍（秒）。
    pub clock_skew_secs: u32,
}

impl TenantTransportProfile {
    /// 由命名套件构造加密侧默认 Profile（`version=1`，TTL/skew=300）。
    ///
    /// # Errors
    ///
    /// [`CryptoError::UnknownSuite`]：未知套件。
    /// [`CryptoError::InvalidProfile`]：`mode` 为 [`TransportMode::None`]。
    pub fn from_named_suite(suite: &str, mode: TransportMode) -> Result<Self, CryptoError> {
        if matches!(mode, TransportMode::None) {
            return Err(CryptoError::InvalidProfile {
                reason: "from_named_suite requires mode request|full".to_owned(),
            });
        }
        let spec = resolve_suite(suite)?;
        Ok(Self {
            version: PROFILE_VERSION_V1,
            mode,
            suite: Some(suite.to_owned()),
            data_alg: Some(spec.data_alg),
            mac_alg: spec.mac_alg,
            key: None,
            require_signature: false,
            allow_insecure: false,
            binding: None,
            nonce_ttl_secs: DEFAULT_NONCE_TTL_SECS,
            clock_skew_secs: DEFAULT_CLOCK_SKEW_SECS,
        })
    }

    /// 明文传输默认 Profile（`mode=none`）。
    #[must_use]
    pub fn plaintext() -> Self {
        Self {
            version: PROFILE_VERSION_V1,
            mode: TransportMode::None,
            suite: None,
            data_alg: None,
            mac_alg: None,
            key: None,
            require_signature: false,
            allow_insecure: false,
            binding: None,
            nonce_ttl_secs: DEFAULT_NONCE_TTL_SECS,
            clock_skew_secs: DEFAULT_CLOCK_SKEW_SECS,
        }
    }

    /// 解析生效的数据 / MAC 算法（合并 suite 与显式字段）。
    ///
    /// # Errors
    ///
    /// 套件未知、字段冲突或加密模式缺少数据算法。
    pub fn effective_algs(
        &self,
    ) -> Result<(Option<AlgorithmId>, Option<AlgorithmId>), CryptoError> {
        let from_suite: Option<SuiteSpec> = match self.suite.as_deref() {
            Some(name) => Some(resolve_suite(name)?),
            None => None,
        };

        if let Some(spec) = from_suite.as_ref() {
            if let Some(data) = self.data_alg
                && data != spec.data_alg
            {
                return Err(CryptoError::InvalidProfile {
                    reason: format!(
                        "data_alg {data} conflicts with suite {} ({})",
                        spec.name, spec.data_alg
                    ),
                });
            }
            if let Some(mac) = self.mac_alg {
                match spec.mac_alg {
                    Some(suite_mac) if mac != suite_mac => {
                        return Err(CryptoError::InvalidProfile {
                            reason: format!(
                                "mac_alg {mac} conflicts with suite {} ({suite_mac})",
                                spec.name
                            ),
                        });
                    }
                    None => {
                        return Err(CryptoError::InvalidProfile {
                            reason: format!("mac_alg {mac} set but suite {} has no mac", spec.name),
                        });
                    }
                    Some(_) => {}
                }
            }
        }

        let data = self
            .data_alg
            .or_else(|| from_suite.as_ref().map(|s| s.data_alg));
        let mac = self
            .mac_alg
            .or_else(|| from_suite.as_ref().and_then(|s| s.mac_alg));
        Ok((data, mac))
    }

    /// 校验 Profile 相对注册表的安全规则。
    ///
    /// # Errors
    ///
    /// - 版本 / TTL 非法
    /// - `mode=none` 却要求签名或配置了加密算法 / suite
    /// - 加密模式缺少数据算法或密钥策略
    /// - 非 AEAD 对称缺 MAC、MAC 种类不对
    /// - Legacy 未 `allow_insecure`
    /// - 算法未登记
    pub fn validate(&self, registry: &AlgorithmRegistry) -> Result<(), CryptoError> {
        if self.version == 0 {
            return Err(CryptoError::InvalidProfile {
                reason: "version must be >= 1".to_owned(),
            });
        }
        if self.nonce_ttl_secs == 0 {
            return Err(CryptoError::InvalidProfile {
                reason: "nonce_ttl_secs must be > 0".to_owned(),
            });
        }
        if self.clock_skew_secs == 0 {
            return Err(CryptoError::InvalidProfile {
                reason: "clock_skew_secs must be > 0".to_owned(),
            });
        }

        let (data_alg, mac_alg) = self.effective_algs()?;

        match self.mode {
            TransportMode::None => {
                if self.require_signature {
                    return Err(CryptoError::InvalidProfile {
                        reason: "mode=none cannot require_signature".to_owned(),
                    });
                }
                if self.suite.is_some() || data_alg.is_some() || mac_alg.is_some() {
                    return Err(CryptoError::InvalidProfile {
                        reason: "mode=none must not set suite/data_alg/mac_alg".to_owned(),
                    });
                }
                return Ok(());
            }
            TransportMode::Request | TransportMode::Full => {}
        }

        let data = data_alg.ok_or_else(|| CryptoError::InvalidProfile {
            reason: "mode=request|full requires data_alg or suite".to_owned(),
        })?;

        let key = self
            .key
            .as_ref()
            .ok_or_else(|| CryptoError::InvalidProfile {
                reason: "mode=request|full requires key selection".to_owned(),
            })?;
        if key.key_id().is_empty() {
            return Err(CryptoError::InvalidProfile {
                reason: "key_id must not be empty".to_owned(),
            });
        }

        let data_meta = registry.get(data)?;
        match data_meta.kind {
            AlgorithmKind::Aead | AlgorithmKind::BlockCipher => {}
            other => {
                return Err(CryptoError::InvalidProfile {
                    reason: format!("data_alg {data} kind {other:?} is not aead|block_cipher"),
                });
            }
        }
        assert_not_legacy_or_allowed(data, self.allow_insecure)?;

        let needs_mac = data_meta.kind == AlgorithmKind::BlockCipher || require_mac_for(data);
        match (needs_mac, mac_alg) {
            (true, None) => {
                return Err(CryptoError::InvalidProfile {
                    reason: format!("data_alg {data} requires mac_alg"),
                });
            }
            (true | false, Some(mac)) => {
                let mac_meta = registry.get(mac)?;
                if mac_meta.kind != AlgorithmKind::Mac {
                    return Err(CryptoError::InvalidProfile {
                        reason: format!("mac_alg {mac} must be a mac algorithm"),
                    });
                }
                assert_not_legacy_or_allowed(mac, self.allow_insecure)?;
            }
            (false, None) => {}
        }

        Ok(())
    }
}
