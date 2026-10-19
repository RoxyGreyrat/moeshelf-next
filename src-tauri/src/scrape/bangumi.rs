//! Bangumi（bgm.tv）数据源。
//!
//! 为什么要它：中文条目名、中文简介、**角色带声优**。
//! VNDB 的角色接口没有 CV，旧库里 814 条声优大多来自 Bangumi。
//!
//! 接口用 v0：
//! - 搜索：`POST /v0/search/subjects`（`filter.type = [4]` 即游戏）
//! - 详情：`GET /v0/subjects/{id}`
//! - 角色：`GET /v0/subjects/{id}/characters`（`actors[].name` 就是声优）
//!
//! ### 注意：这个源在某些网络环境下会连不上（DNS 能解析但连接被重置）。
//! 失败时前端会给出明确提示，不影响 VNDB / 月幕继续用。

use serde_json::Value;

use super::Candidate;
use crate::models::Character;

const BASE: &str = "https://api.bgm.tv";

/// Bangumi 要求 UA 里带联系方式，否则可能被拒
fn ua() -> Vec<(&'static str, String)> {
    vec![
        (
            "User-Agent",
            "MoeShelfNext/0.1 (local desktop app)".to_string(),
        ),
        ("Accept", "application/json".to_string()),
    ]
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .map(|x| match x {
            Value::String(t) => t.trim().to_string(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

/// 图片地址在接口里是 `//lain.bgm.tv/...`，要补协议
fn image_url(v: &Value, key: &str) -> Option<String> {
    let raw = v
        .get("images")
        .and_then(|i| i.get(key))
        .and_then(|u| u.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if raw.is_empty() {
        return None;
    }
    Some(if raw.starts_with("//") {
        format!("https:{raw}")
    } else {
        raw
    })
}

fn to_candidate(v: &Value) -> Candidate {
    Candidate {
        source: "bangumi".to_string(),
        id: s(v, "id"),
        title: s(v, "name"),
        title_cn: s(v, "name_cn"),
        alt_title: s(v, "name"),
        developer: String::new(),
        released: s(v, "date"),
        rating: v
            .get("rating")
            .map(|r| num(r, "score"))
            .unwrap_or(0.0),
        description: String::new(),
        // Bangumi 的 summary 是中文
        description_cn: s(v, "summary"),
        image_url: image_url(v, "large"),
    }
}

pub fn search(query: &str, limit: u32) -> Result<Vec<Candidate>, String> {
    let body = serde_json::json!({
        "keyword": query,
        "filter": { "type": [4] },
    });

    let resp = super::client()?
        .post(format!("{BASE}/v0/search/subjects"))
        .header("User-Agent", "MoeShelfNext/0.1 (local desktop app)")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("连不上 Bangumi（{e}）。这个源需要能访问 api.bgm.tv，可先换用 VNDB 或月幕。"))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败：{e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Bangumi 返回 HTTP {status}：{}",
            text.chars().take(160).collect::<String>()
        ));
    }

    let v: Value = serde_json::from_str(&text).map_err(|e| format!("解析响应失败：{e}"))?;
    let list = v
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(list
        .iter()
        .take(limit.clamp(1, 25) as usize)
        .map(to_candidate)
        .collect())
}

/// 从 infobox 里取"开发"（值可能是字符串，也可能是 `[{v: "..."}]`）
fn developer_from(v: &Value) -> String {
    let Some(list) = v.get("infobox").and_then(|x| x.as_array()) else {
        return String::new();
    };

    for item in list {
        let key = s(item, "key");
        if key != "开发" && key != "开发商" && key != "开发公司" {
            continue;
        }
        match item.get("value") {
            Some(Value::String(t)) => return t.trim().to_string(),
            Some(Value::Array(arr)) => {
                let names: Vec<String> = arr
                    .iter()
                    .filter_map(|x| x.get("v").and_then(|v| v.as_str()))
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect();
                if !names.is_empty() {
                    return names.join(" / ");
                }
            }
            _ => {}
        }
    }
    String::new()
}

pub fn detail(id: &str) -> Result<(Candidate, Vec<Character>), String> {
    let v = super::get_json(&format!("{BASE}/v0/subjects/{id}"), &ua())
        .map_err(|e| format!("连不上 Bangumi：{e}"))?;

    let mut candidate = to_candidate(&v);
    candidate.developer = developer_from(&v);
    // 详情里的 summary 更完整
    let summary = s(&v, "summary");
    if !summary.is_empty() {
        candidate.description_cn = summary;
    }

    // 角色拿不到就只补资料，不算失败
    let characters = fetch_characters(id).unwrap_or_default();
    Ok((candidate, characters))
}

pub fn fetch_characters(id: &str) -> Result<Vec<Character>, String> {
    let v = super::get_json(&format!("{BASE}/v0/subjects/{id}/characters"), &ua())?;
    let list = v.as_array().cloned().unwrap_or_default();

    let mut out: Vec<Character> = Vec::new();
    for item in list.iter() {
        let name = s(item, "name");
        if name.is_empty() {
            continue;
        }

        // 声优：actors 里第一个
        let cv = item
            .get("actors")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .map(|x| s(x, "name"))
            .unwrap_or_default();

        // relation：1 主角 / 2 配角 / 3 客串
        let role = match item.get("relation").and_then(|r| r.as_i64()).unwrap_or(0) {
            1 => "主角",
            2 => "次要人物",
            3 => "仅出现",
            _ => "",
        };

        out.push(Character {
            name,
            aliases: Vec::new(),
            role: role.to_string(),
            cv,
            cv_source: "bangumi".to_string(),
            image_url: image_url(item, "grid"),
            sort_order: out.len() as i64,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subject() {
        let v = serde_json::json!({
            "id": 253332,
            "name": "千恋＊万花",
            "name_cn": "千恋＊万花",
            "date": "2016-07-29",
            "summary": "中文简介",
            "images": { "large": "//lain.bgm.tv/pic/cover/l/xx.jpg" },
            "rating": { "score": 8.5 }
        });
        let c = to_candidate(&v);
        assert_eq!(c.id, "253332");
        assert_eq!(c.title_cn, "千恋＊万花");
        assert_eq!(c.rating, 8.5);
        assert_eq!(c.description_cn, "中文简介");
        assert_eq!(c.image_url.as_deref(), Some("https://lain.bgm.tv/pic/cover/l/xx.jpg"));
    }

    #[test]
    fn reads_developer_from_infobox() {
        let v = serde_json::json!({
            "infobox": [
                { "key": "中文名", "value": "千恋万花" },
                { "key": "开发", "value": [{ "v": "ゆずソフト" }] }
            ]
        });
        assert_eq!(developer_from(&v), "ゆずソフト");

        let plain = serde_json::json!({ "infobox": [{ "key": "开发", "value": "Yuzusoft" }] });
        assert_eq!(developer_from(&plain), "Yuzusoft");

        assert_eq!(developer_from(&serde_json::json!({})), "");
    }
}
