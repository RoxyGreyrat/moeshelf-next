//! 连通性探针：给设置页的「测试连接」用。
//!
//! 只做一件事 —— 用**当前（或待保存）的代理设置**去打三个源各自的轻量接口，
//! 把结果原样报给用户。不重试、不吞错：用户点这个按钮就是为了看真实原因。
//!
//! 为什么必须能带"待保存"的代理测：让用户先验证再保存，否则改一次代理、
//! 保存、去获取资料失败、再回来改，来回三趟。

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use super::client_for_probe;

/// 单个源的探测结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    /// vndb / bangumi / ymgal
    pub source: String,
    /// 界面上显示的名字
    pub label: String,
    pub ok: bool,
    pub ms: u64,
    /// 失败原因（成功时为空）
    pub message: String,
}

const TIMEOUT: Duration = Duration::from_secs(8);

pub fn run(proxy: Option<&str>) -> Vec<ProbeResult> {
    vec![
        probe("vndb", "VNDB", proxy, check_vndb),
        probe("bangumi", "Bangumi", proxy, check_bangumi),
        probe("ymgal", "月幕", proxy, check_ymgal),
    ]
}

/// 每个源单独建客户端：一个源的连接池被卡住不该影响另外两个的计时。
fn probe(
    source: &str,
    label: &str,
    proxy: Option<&str>,
    check: fn(&reqwest::blocking::Client) -> Result<(), String>,
) -> ProbeResult {
    let started = Instant::now();

    let result = client_for_probe(proxy).and_then(|client| check(&client));

    match result {
        Ok(()) => ProbeResult {
            source: source.to_string(),
            label: label.to_string(),
            ok: true,
            ms: started.elapsed().as_millis() as u64,
            message: String::new(),
        },
        Err(message) => ProbeResult {
            source: source.to_string(),
            label: label.to_string(),
            ok: false,
            ms: started.elapsed().as_millis() as u64,
            message,
        },
    }
}

/// 把 reqwest 的英文错误翻成一句人话 —— 用户不需要看 hyper 的调用栈
fn humanize(err: &reqwest::Error) -> String {
    let raw = err.to_string();
    let lower = raw.to_lowercase();

    if err.is_timeout() || lower.contains("timed out") {
        "连接超时，代理没生效或链路太慢".to_string()
    } else if lower.contains("connection refused") || lower.contains("os error 10061") {
        "连接被拒绝，代理端口没在监听".to_string()
    } else if lower.contains("dns") || lower.contains("failed to lookup") {
        "域名解析失败，检查代理是否开启了 DNS".to_string()
    } else if lower.contains("certificate") || lower.contains("tls") {
        "TLS 握手失败，可能是代理在做证书拦截".to_string()
    } else if lower.contains("connection reset") || lower.contains("os error 10054") {
        "连接被重置，这个域名可能被网络中间设备阻断".to_string()
    } else {
        raw.chars().take(160).collect()
    }
}

fn check_vndb(client: &reqwest::blocking::Client) -> Result<(), String> {
    let body = serde_json::json!({
        "filters": ["id", "=", "v17"],
        "fields": "id,title",
        "results": 1,
    });

    let resp = client
        .post("https://api.vndb.org/kana/vn")
        .timeout(TIMEOUT)
        .json(&body)
        .send()
        .map_err(|e| humanize(&e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("接口返回 HTTP {status}"));
    }

    let v: Value = resp.json().map_err(|e| format!("响应不是 JSON：{e}"))?;
    match v
        .get("results")
        .and_then(|r| r.as_array())
        .and_then(|list| list.first())
        .and_then(|item| item.get("id"))
        .and_then(|id| id.as_str())
    {
        Some("v17") => Ok(()),
        _ => Err("接口有响应，但没返回预期条目".to_string()),
    }
}

fn check_bangumi(client: &reqwest::blocking::Client) -> Result<(), String> {
    let resp = client
        .get("https://api.bgm.tv/v0/subjects/253332")
        .timeout(TIMEOUT)
        .header(
            "User-Agent",
            "MoeShelfNext/0.1 (local desktop app; https://github.com/)",
        )
        .header("Accept", "application/json")
        .send()
        .map_err(|e| humanize(&e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("接口返回 HTTP {status}"));
    }
    Ok(())
}

fn check_ymgal(client: &reqwest::blocking::Client) -> Result<(), String> {
    let resp = client
        .post("https://www.ymgal.games/oauth/token")
        .timeout(TIMEOUT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("grant_type=client_credentials&client_id=ymgal&client_secret=luna0327&scope=public")
        .send()
        .map_err(|e| humanize(&e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("鉴权接口返回 HTTP {status}"));
    }

    let v: Value = resp.json().map_err(|e| format!("响应不是 JSON：{e}"))?;
    if v.get("access_token").and_then(|t| t.as_str()).is_some() {
        Ok(())
    } else {
        Err("鉴权没有返回 token".to_string())
    }
}
