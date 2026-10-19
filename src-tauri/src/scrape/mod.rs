//! 网络抓取。
//!
//! 这里只放"怎么发请求"，不放业务规则 —— 每个数据源一个文件（现在只有 VNDB）。
//!
//! 两条硬约束：
//! 1. 全部走 `spawn_blocking`：reqwest 的阻塞客户端不能跑在 async 运行时线程上；
//! 2. 超时给足（20s）但必须有 —— 没超时的请求会让 UI 永远停在"刮削中"。

pub mod vndb;

use std::time::Duration;

/// 统一的 HTTP 客户端。
///
/// 不显式设置代理：reqwest 默认读 `HTTP_PROXY` / `HTTPS_PROXY` 与系统代理，
/// 用户开着梯子就能连上 VNDB，关掉就走直连。以后要做"应用内代理设置"再在这里改。
fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("MoeShelfNext/0.1 (local desktop app)")
        .build()
        .map_err(|e| format!("创建网络客户端失败：{e}"))
}

/// GET 一个二进制资源（封面、角色立绘）
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = client()?
        .get(url)
        .send()
        .map_err(|e| format!("下载失败：{e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("下载失败：HTTP {status}"));
    }

    let bytes = resp.bytes().map_err(|e| format!("读取响应失败：{e}"))?;
    Ok(bytes.to_vec())
}

/// POST 一个 JSON，返回解析后的 JSON
pub fn post_json(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let resp = client()?
        .post(url)
        .json(body)
        .send()
        .map_err(|e| format!("请求失败：{e}"))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败：{e}"))?;

    if !status.is_success() {
        let snippet: String = text.chars().take(240).collect();
        return Err(format!("服务返回 HTTP {status}：{snippet}"));
    }

    serde_json::from_str(&text).map_err(|e| format!("解析响应失败：{e}"))
}
