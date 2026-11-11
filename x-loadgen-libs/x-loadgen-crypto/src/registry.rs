//! 算法注册表：按 id 查询元数据与本步已实现的原语句柄。
//!
//! 尚未登记的闭集 id 返回 [`CryptoError::NotRegistered`]（明确错误，非静默回退）。

use std::collections::BTreeMap;

use crate::aead::AeadAlgorithm;
use crate::asym::AsymSignAlgorithm;
use crate::id::{AlgorithmId, AlgorithmKind};
use crate::mac::MacAlgorithm;
use crate::security::SecurityClass;
use crate::sm::{Sm2SignAlgorithm, Sm4Algorithm};
use crate::symmetric::SymmetricAlgorithm;
use crate::traits::CryptoError;

/// CBC / CTR / CFB 等非 AEAD 对称模式是否**必须**在画像层配置 MAC。
///
/// 强制组合校验在 Profile 层（step-04/09）；本函数仅提供元数据。
#[must_use]
pub const fn require_mac_for(id: AlgorithmId) -> bool {
    matches!(
        id,
        AlgorithmId::Aes256Cbc
            | AlgorithmId::Aes128Cbc
            | AlgorithmId::Aes256Ctr
            | AlgorithmId::Aes128Ctr
            | AlgorithmId::Aes256Cfb
            | AlgorithmId::Aes128Cfb
            | AlgorithmId::Sm4CbcPkcs7
            | AlgorithmId::Sm4Ctr
    )
}

/// 注册表中的算法元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorithmMeta {
    /// 算法 id。
    pub id: AlgorithmId,
    /// 安全分级。
    pub security: SecurityClass,
    /// 原语种类。
    pub kind: AlgorithmKind,
    /// 画像层是否必须外挂 MAC（CBC/CTR/CFB 为 true）。
    pub require_mac: bool,
}

impl AlgorithmMeta {
    /// 由 id 推导安全分级、种类与 `require_mac`。
    #[must_use]
    pub const fn from_id(id: AlgorithmId) -> Self {
        Self {
            id,
            security: id.security_class(),
            kind: id.kind(),
            require_mac: require_mac_for(id),
        }
    }
}

/// 算法注册表。
///
/// 空表合法；`get` 对未登记 id 返回错误。闭集枚举见 [`AlgorithmId::all`]。
#[derive(Debug, Default, Clone)]
pub struct AlgorithmRegistry {
    entries: BTreeMap<AlgorithmId, AlgorithmMeta>,
}

impl AlgorithmRegistry {
    /// 空注册表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// step-03 默认注册表：闭集全部 id（含国密 / 非对称 / legacy）。
    ///
    /// SM9 **不**在闭集中；见 [`parse_algorithm_id`]。
    #[must_use]
    pub fn default_registry() -> Self {
        let mut registry = Self::new();
        for id in AlgorithmId::all() {
            registry.register_id(*id);
        }
        registry
    }

    /// 登记元数据；同 id 重复登记覆盖。
    pub fn register(&mut self, meta: AlgorithmMeta) {
        self.entries.insert(meta.id, meta);
    }

    /// 按闭集 id 登记默认元数据。
    pub fn register_id(&mut self, id: AlgorithmId) {
        self.register(AlgorithmMeta::from_id(id));
    }

    /// 查询已登记元数据。
    ///
    /// # Errors
    ///
    /// [`CryptoError::NotRegistered`]：id 尚未 `register`。
    pub fn get(&self, id: AlgorithmId) -> Result<&AlgorithmMeta, CryptoError> {
        self.entries
            .get(&id)
            .ok_or(CryptoError::NotRegistered { id })
    }

    /// 是否已登记。
    #[must_use]
    pub fn contains(&self, id: AlgorithmId) -> bool {
        self.entries.contains_key(&id)
    }

    /// 已登记条目数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 已登记 id 列表（按 `AlgorithmId` 判别序）。
    #[must_use]
    pub fn registered_ids(&self) -> Vec<AlgorithmId> {
        self.entries.keys().copied().collect()
    }

    /// 平台闭集全部 id（与是否已登记无关）。
    #[must_use]
    pub fn closed_set() -> &'static [AlgorithmId] {
        AlgorithmId::all()
    }

    /// 取得已登记 AEAD 实现句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]：已登记但非 AEAD 实现
    pub fn aead(&self, id: AlgorithmId) -> Result<AeadAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        AeadAlgorithm::from_id(id)
    }

    /// 取得已登记对称（非 AEAD）实现句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn block_cipher(&self, id: AlgorithmId) -> Result<SymmetricAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        SymmetricAlgorithm::from_id(id)
    }

    /// 取得已登记 MAC 实现句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn mac(&self, id: AlgorithmId) -> Result<MacAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        MacAlgorithm::from_id(id)
    }

    /// 查询 SM4（非 ECB）句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn sm4(&self, id: AlgorithmId) -> Result<Sm4Algorithm, CryptoError> {
        let _ = self.get(id)?;
        Sm4Algorithm::from_id(id)
    }

    /// 查询非对称签名句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn asym_sign(&self, id: AlgorithmId) -> Result<AsymSignAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        AsymSignAlgorithm::from_id(id)
    }

    /// 查询 SM2 签名句柄（默认 `user_id`）。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn sm2_sign(&self, id: AlgorithmId) -> Result<Sm2SignAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        Sm2SignAlgorithm::from_id(id)
    }
}

/// 解析算法 id 字符串。
///
/// - 闭集 kebab-case → [`AlgorithmId`]
/// - 以 `sm9-` 开头 → [`CryptoError::Unsupported`]（明确不做）
/// - 其它 → [`CryptoError::UnknownAlgorithmId`]
///
/// # Errors
///
/// 见上。
pub fn parse_algorithm_id(raw: &str) -> Result<AlgorithmId, CryptoError> {
    if raw.starts_with("sm9-") || raw == "sm9" {
        return Err(CryptoError::Unsupported {
            reason: format!("SM9 is not implemented: {raw}"),
        });
    }
    raw.parse()
}
