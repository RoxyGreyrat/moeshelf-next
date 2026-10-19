//! VNDB（kana API）客户端。
//!
//! 只实现两件事：**按标题搜候选**、**按 id 取详情（含角色）**。
//! 评分、厂商、发售日、封面、角色都能从这两个接口拿到，
//! 中文标题取 `titles` 里 `lang == "zh-Hans"` 的那条（VNDB 近年陆续补了官方中文名）。
//!
//! 限流：VNDB 要求每秒最多 1 个请求。一次"应用"最多发 2 个（详情 + 角色），
//! 所以两次调用之间 sleep 300ms 就够，不必做全局令牌桶。

use std::collections::HashSet;
use std::thread::sleep;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;

use crate::models::Character;

use super::Candidate;

const API_VN: &str = "https://api.vndb.org/kana/vn";
const API_CHAR: &str = "https://api.vndb.org/kana/character";

const VN_FIELDS: &str = "id,title,alttitle,titles{lang,title,latin,official},image.url,released,rating,developers{name},description,tags{name,rating,spoiler,category}";
const CHAR_FIELDS: &str = "name,original,aliases,image.url,role";

/// 搜索结果里的一条候选
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VnCandidate {
    pub id: String,
    pub title: String,
    /// 原文名（日文）
    pub alt_title: String,
    /// VNDB 的官方中文名，没有就是空串
    pub title_cn: String,
    pub image_url: Option<String>,
    pub released: String,
    /// 已归一到 0~10
    pub rating: f64,
    pub developer: String,
    pub description: String,
}

pub fn value_str(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn num(v: &serde_json::Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

/// 从 `titles` 里挑官方中文名
pub fn zh_title(vn: &serde_json::Value) -> String {
    let Some(list) = vn.get("titles").and_then(|x| x.as_array()) else {
        return String::new();
    };
    // 简体优先，其次繁体
    for lang in ["zh-Hans", "zh-Hant", "zh"] {
        if let Some(item) = list.iter().find(|t| value_str(t, "lang") == lang) {
            let title = value_str(item, "title");
            if !title.is_empty() {
                return title;
            }
        }
    }
    String::new()
}

pub fn first_developer(vn: &serde_json::Value) -> String {
    vn.get("developers")
        .and_then(|x| x.as_array())
        .and_then(|list| list.first())
        .map(|d| value_str(d, "name"))
        .unwrap_or_default()
}

fn to_candidate(vn: &serde_json::Value) -> VnCandidate {
    VnCandidate {
        id: value_str(vn, "id"),
        title: value_str(vn, "title"),
        alt_title: value_str(vn, "alttitle"),
        title_cn: zh_title(vn),
        image_url: vn
            .get("image")
            .and_then(|i| i.get("url"))
            .and_then(|u| u.as_str())
            .map(|u| u.to_string())
            .filter(|u| !u.is_empty()),
        released: value_str(vn, "released"),
        rating: (num(vn, "rating") / 10.0 * 10.0).round() / 10.0,
        developer: first_developer(vn),
        description: value_str(vn, "description"),
    }
}

/// 按标题搜候选。`limit` 建议 8~12：再多用户也挑不动，而且 VNDB 会慢。
impl VnCandidate {
    /// 转成跨源统一的候选结构
    pub fn into_candidate(self) -> Candidate {
        Candidate {
            source: "vndb".to_string(),
            id: self.id,
            title: self.title,
            title_cn: self.title_cn,
            alt_title: self.alt_title,
            developer: self.developer,
            released: self.released,
            rating: self.rating,
            description: self.description,
            description_cn: String::new(),
            image_url: self.image_url,
        }
    }
}

/// 直接从 VN 原始 JSON 造统一候选（详情接口用，避免再包一层 VnCandidate）
pub fn candidate_from_vn(vn: &serde_json::Value) -> Candidate {
    to_candidate(vn).into_candidate()
}

pub fn search(query: &str, limit: u32) -> Result<Vec<VnCandidate>, String> {
    let body = json!({
        "filters": ["search", "=", query],
        "fields": VN_FIELDS,
        "sort": "searchrank",
        "results": limit.clamp(1, 25),
    });

    let v = super::post_json(API_VN, &body)?;
    let results = v
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(results.iter().map(to_candidate).collect())
}

/// 按 id 取单条详情。`id` 形如 `v17`，也接受纯数字。
pub fn fetch_vn(id: &str) -> Result<serde_json::Value, String> {
    let id = if id.starts_with('v') {
        id.to_string()
    } else {
        format!("v{id}")
    };

    let body = json!({
        "filters": ["id", "=", id],
        "fields": VN_FIELDS,
        "results": 1,
    });

    let v = super::post_json(API_VN, &body)?;
    v.get("results")
        .and_then(|r| r.as_array())
        .and_then(|list| list.first())
        .cloned()
        .ok_or_else(|| format!("VNDB 上找不到 {id}"))
}

/// 取一部作品的角色表（含头像，没有声优 —— VNDB 的角色接口不返回 CV）
pub fn fetch_characters(vn_id: &str, locked: &HashSet<String>) -> Result<Vec<Character>, String> {
    let id = if vn_id.starts_with('v') {
        vn_id.to_string()
    } else {
        format!("v{vn_id}")
    };

    let body = json!({
        "filters": ["vn", "=", ["id", "=", id]],
        "fields": CHAR_FIELDS,
        "results": 100,
    });

    let v = super::post_json(API_CHAR, &body)?;
    let list = v
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    // 角色顺序：主角 → 主要 → 次要 → 客串，同级别保持接口顺序
    let weight = |role: &str| match role {
        "main" => 0,
        "primary" => 1,
        "side" => 2,
        _ => 3,
    };

    let mut items: Vec<(usize, Character)> = list
        .iter()
        .filter_map(|c| {
            let name = value_str(c, "name");
            if name.is_empty() {
                return None;
            }
            let role_en = value_str(c, "role");
            Some((
                weight(&role_en),
                Character {
                    name,
                    aliases: c
                        .get("aliases")
                        .and_then(|a| a.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str())
                                .map(|x| x.trim().to_string())
                                .filter(|x| !x.is_empty())
                                .collect()
                        })
                        .unwrap_or_default(),
                    role: match role_en.as_str() {
                        "main" => "主角",
                        "primary" => "主要人物",
                        "side" => "次要人物",
                        _ => "仅出现",
                    }
                    .to_string(),
                    cv: String::new(),
                    cv_source: String::new(),
                    image_url: c
                        .get("image")
                        .and_then(|i| i.get("url"))
                        .and_then(|u| u.as_str())
                        .map(|u| u.to_string())
                        .filter(|u| !u.is_empty()),
                    sort_order: 0,
                },
            ))
        })
        .collect();

    items.sort_by_key(|(w, _)| *w);
    let mut out: Vec<Character> = items.into_iter().map(|(_, c)| c).collect();
    for (i, c) in out.iter_mut().enumerate() {
        c.sort_order = i as i64;
    }

    // 声优不覆盖：旧库从 Bangumi / cngal 补过一部分，VNDB 这边本来就没有，
    // 保留旧数据比清空更有价值（`locked` 里含 "cv" 时更不该动）。
    let _ = locked;

    Ok(out)
}

/// 两次请求之间歇一下。VNDB 建议 1 req/s，这里留 300ms 是因为一次刮削最多连发两枪。
pub fn polite_pause() {
    sleep(Duration::from_millis(300));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "id": "v20230",
            "title": "Senren * Banka",
            "alttitle": "千恋＊万花",
            "titles": [
                { "lang": "ja", "title": "千恋＊万花", "official": true },
                { "lang": "zh-Hans", "title": "千恋＊万花", "official": true }
            ],
            "image": { "url": "https://t.vndb.org/cv/46/80746.jpg" },
            "released": "2016-07-29",
            "rating": 68.4,
            "developers": [{ "name": "Yuzusoft" }],
            "description": "A story about a sword.",
            "tags": [
                { "name": "Nakige", "rating": 2.5, "spoiler": 0 },
                { "name": "Spoiler Tag", "rating": 3.0, "spoiler": 1 },
                { "name": "Low Weight", "rating": 1.0, "spoiler": 0 }
            ]
        })
    }

    #[test]
    fn parses_candidate_fields() {
        let c = to_candidate(&sample());
        assert_eq!(c.id, "v20230");
        assert_eq!(c.title_cn, "千恋＊万花");
        assert_eq!(c.alt_title, "千恋＊万花");
        assert_eq!(c.developer, "Yuzusoft");
        assert_eq!(c.released, "2016-07-29");
        // VNDB 的 68.4 → 新库的 6.8
        assert_eq!(c.rating, 6.8);
        assert_eq!(c.image_url.as_deref(), Some("https://t.vndb.org/cv/46/80746.jpg"));
    }

    #[test]
    fn zh_title_prefers_simplified() {
        let vn = serde_json::json!({
            "titles": [
                { "lang": "zh-Hant", "title": "千戀＊萬花" },
                { "lang": "zh-Hans", "title": "千恋＊万花" }
            ]
        });
        assert_eq!(zh_title(&vn), "千恋＊万花");
    }

    #[test]
    fn missing_fields_do_not_panic() {
        let c = to_candidate(&serde_json::json!({}));
        assert!(c.id.is_empty());
        assert_eq!(c.rating, 0.0);
    }
}
