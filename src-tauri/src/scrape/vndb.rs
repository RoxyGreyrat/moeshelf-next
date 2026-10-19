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
const API_RELEASE: &str = "https://api.vndb.org/kana/release";

const VN_FIELDS: &str = "id,title,alttitle,titles{lang,title,latin,official},image.url,released,rating,developers{name},description,tags{name,rating,spoiler,category}";
const CHAR_FIELDS: &str = "name,original,aliases,image.url,role";
const RELEASE_FIELDS: &str = "id,title,images{id,url,type,dims,sexual}";

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

/// `17` / `v17` 都接受，统一成 `v17`
pub fn normalize_id(id: &str) -> String {
    let t = id.trim();
    if t.starts_with('v') {
        t.to_string()
    } else {
        format!("v{t}")
    }
}

/// 按 id 取单条详情。`id` 形如 `v17`，也接受纯数字。
pub fn fetch_vn(id: &str) -> Result<serde_json::Value, String> {
    let id = normalize_id(id);

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

/// 一个 release（发售版本）上的图片，就是"同一部作品的多张封面"的真实来源。
///
/// VNDB 的 VN 条目本身只有一张 `image`，但它的每个发售版本（初回限定、通常版、
/// 数字版、主机移植版…）各自带自己的包装图。旧项目换封面也是在这一层挑。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCover {
    pub url: String,
    pub label: String,
    /// 中文类型：包装正面 / 数字版 / 包装内容 …
    pub kind: String,
}

/// 图片类型 →（排序权重，中文名）。数字越小越像"封面"。
fn kind_meta(raw: &str) -> (u8, &'static str) {
    match raw {
        "pkgfront" => (0, "包装正面"),
        "dig" => (1, "数字版"),
        "pkgcontent" => (2, "包装内容"),
        "pkgside" => (3, "包装侧面"),
        "pkgback" => (4, "包装背面"),
        _ => (5, "图片"),
    }
}

/// 取一部作品所有发售版本上的图片。
///
/// 会过滤掉明显的横图（`pkgback` 常是横排的封底拼接图），并做一次 URL 去重 ——
/// 同一个封面在多个版本里重复出现是常态。`limit` 建议 20~40：再多用户也不会翻。
pub fn fetch_release_covers(vn_id: &str, limit: u32) -> Result<Vec<ReleaseCover>, String> {
    let id = normalize_id(vn_id);

    let body = json!({
        "filters": ["vn", "=", ["id", "=", id]],
        "fields": RELEASE_FIELDS,
        "results": 100,
    });

    let v = super::post_json(API_RELEASE, &body)?;
    let releases = v
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let mut scored: Vec<(u8, ReleaseCover)> = Vec::new();

    for rel in &releases {
        let rel_title = value_str(rel, "title");
        let Some(images) = rel.get("images").and_then(|i| i.as_array()) else {
            continue;
        };

        for img in images {
            let url = value_str(img, "url");
            if url.is_empty() {
                continue;
            }

            // 横图不当封面：封底拼接图、特典横幅混进竖版网格里很难看
            let (w, h) = img
                .get("dims")
                .and_then(|d| d.as_array())
                .map(|d| {
                    (
                        d.first().and_then(|x| x.as_u64()).unwrap_or(0),
                        d.get(1).and_then(|x| x.as_u64()).unwrap_or(0),
                    )
                })
                .unwrap_or((0, 0));
            if w > 0 && h > 0 && w * 4 > h * 5 {
                continue;
            }

            let (weight, kind) = kind_meta(&value_str(img, "type"));
            let label = if rel_title.is_empty() {
                kind.to_string()
            } else {
                format!("{rel_title} · {kind}")
            };

            scored.push((
                weight,
                ReleaseCover {
                    url,
                    label,
                    kind: kind.to_string(),
                },
            ));
        }
    }

    // 稳定的按类型排：同类型内保持接口顺序（同一版本的多张图不会乱跳）
    scored.sort_by_key(|(w, _)| *w);

    let mut seen = HashSet::new();
    let list: Vec<ReleaseCover> = scored
        .into_iter()
        .filter(|(_, c)| seen.insert(c.url.clone()))
        .map(|(_, c)| c)
        .take(limit.clamp(1, 60) as usize)
        .collect();

    Ok(list)
}

/// 取一部作品的角色表（含头像，没有声优 —— VNDB 的角色接口不返回 CV）
pub fn fetch_characters(vn_id: &str, locked: &HashSet<String>) -> Result<Vec<Character>, String> {
    let id = normalize_id(vn_id);

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

    #[test]
    fn kind_meta_orders_cover_like_types() {
        assert_eq!(kind_meta("pkgfront").0, 0);
        assert_eq!(kind_meta("dig").0, 1);
        assert!(kind_meta("pkgback").0 > kind_meta("pkgcontent").0);
        assert_eq!(kind_meta("unknown-type").1, "图片");
    }

    /// 真机联网验证：千恋万花（v19073）23 个 release 一共 10 张图，
    /// 去掉横图和重复后应该还有好几张竖版封面。
    ///
    /// `cargo test live_release_covers -- --ignored --nocapture`（需要能连 VNDB）
    #[test]
    #[ignore]
    fn live_release_covers() {
        let list = fetch_release_covers("v19073", 30).expect("请求 VNDB 失败");
        for c in &list {
            println!("{}｜{}", c.kind, c.label);
        }
        assert!(list.len() >= 3, "应该拿到多张封面，实际 {}", list.len());
        assert_eq!(list[0].kind, "包装正面", "包装正面要排在第一位");
        assert!(
            list.iter().all(|c| c.url.starts_with("https://")),
            "只接受 https 图床"
        );
    }
}
