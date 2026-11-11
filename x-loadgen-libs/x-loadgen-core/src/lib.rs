//! x-loadgen 的 mazbri 线上格式编解码：传输信封（`crypto::transport`）、
//! HTTP 请求绑定签名（`crypto::request_binding`）与饱和数值转换（`num`）。
//!
//! 签名串、头名、信封 JSON 字段与命名套件均与 mazbri 平台一致（算法实现经 `x-loadgen-crypto`）。
pub mod crypto;
pub mod num;
