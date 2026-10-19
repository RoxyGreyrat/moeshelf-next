//! 网络抓取。
//!
//! 结构：每个数据源一个文件，`search` / `detail` 两个入口，统一产出 [`Candidate`]。
//! 上层（commands）不关心具体是哪个源，只按 `source` 字符串分发。
//!
//! 两条硬约束：
//! 1. 全部走 `spawn_blocking`：reqwest 的阻塞客户端不能跑在 async 运行时线程上；
//! 2. 超时给足但必须有 —— 没超时的请求会让界面永远停在"查询中"。

pub mod bangumi;
pub mod probe;
pub mod vndb;
pub mod ymgal;

use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use serde::Serialize;

use crate::models::Character;

/// `app_settings` 里存代理地址的键。空串 = 自动（系统代理 / 环境变量）。
pub const PROXY_KEY: &str = "network.proxy";

/// 当前生效的代理。启动时从库里读一次，设置页保存时改写。
///
/// 用全局量而不是每次请求查库：`client()` 会被封面下载、角色立绘、三个源的
/// 每一次请求调用，查库既慢又要把连接锁来锁去。
static PROXY: OnceLock<RwLock<String>> = OnceLock::new();

fn proxy_slot() -> &'static RwLock<String> {
    PROXY.get_or_init(|| RwLock::new(String::new()))
}

pub fn set_proxy(url: &str) {
    if let Ok(mut slot) = proxy_slot().write() {
        *slot = url.trim().to_string();
    }
}

pub fn proxy() -> String {
    proxy_slot()
        .read()
        .map(|slot| slot.clone())
        .unwrap_or_default()
}

/// 一个候选条目。三个源的字段取并集，缺的留空 —— 前端只按这几列渲染。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// `vndb` / `bangumi` / `ymgal`
    pub source: String,
    /// 该源里的 id（v19073 / 253332 / 22374）
    pub id: String,
    /// 主标题（原文或罗马字）
    pub title: String,
    /// 中文名
    pub title_cn: String,
    /// 原文名（日文）
    pub alt_title: String,
    pub developer: String,
    pub released: String,
    /// 统一 0~10
    pub rating: f64,
    /// 外文简介（VNDB 是英文）
    pub description: String,
    /// 中文简介（月幕 / Bangumi 提供）
    pub description_cn: String,
    pub image_url: Option<String>,
}

/// 统一的 HTTP 客户端。
///
/// 代理行为：
/// - 用户在设置页填了地址 → 所有请求走它（此时 reqwest 不再自动叠加系统代理）；
/// - 留空 → 保持 reqwest 的自动行为：读 `HTTPS_PROXY` 等环境变量 + 系统代理设置。
///
/// 为什么要给手填入口：Windows 的系统代理若是 PAC / TUN 模式，系统接口拿不到
/// 具体的 host:port，reqwest 就只能直连 —— 表现就是"明明开着梯子，Bangumi 还是连不上"。
fn client_with_proxy(explicit: Option<&str>) -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("MoeShelfNext/0.1 (local desktop app)");

    let target = explicit.map(str::trim).filter(|s| !s.is_empty());
    if let Some(url) = target {
        let proxy = reqwest::Proxy::all(url).map_err(|e| format!("代理地址不可用：{e}"))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("创建网络客户端失败：{e}"))
}

fn client() -> Result<reqwest::blocking::Client, String> {
    let configured = proxy();
    if configured.is_empty() {
        client_with_proxy(None)
    } else {
        client_with_proxy(Some(&configured))
    }
}

/// 探针用：临时指定代理，不读全局设置（用户还没点保存也能先测）。
pub fn client_for_probe(proxy: Option<&str>) -> Result<reqwest::blocking::Client, String> {
    client_with_proxy(proxy)
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
        .header("Accept", "application/json")
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

/// GET 一个 JSON
pub fn get_json(url: &str, headers: &[(&str, String)]) -> Result<serde_json::Value, String> {
    let mut req = client()?.get(url).header("Accept", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req.send().map_err(|e| format!("请求失败：{e}"))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败：{e}"))?;
    if !status.is_success() {
        let snippet: String = text.chars().take(240).collect();
        return Err(format!("服务返回 HTTP {status}：{snippet}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("解析响应失败：{e}"))
}

/// 搜候选。`source` 只接受 [`SOURCES`] 里的值。
pub fn search(source: &str, query: &str, limit: u32) -> Result<Vec<Candidate>, String> {
    match source {
        "vndb" => Ok(vndb::search(query, limit)?
            .into_iter()
            .map(|c| c.into_candidate())
            .collect()),
        "bangumi" => bangumi::search(query, limit),
        "ymgal" => ymgal::search(query, limit),
        other => Err(format!("未知的信息源：{other}")),
    }
}

/// 取详情（含角色）。详情里的简介通常比搜索结果完整。
pub fn detail(source: &str, id: &str) -> Result<(Candidate, Vec<Character>), String> {
    match source {
        "vndb" => {
            let vn = vndb::fetch_vn(id)?;
            let candidate = vndb::candidate_from_vn(&vn);
            // VNDB 要求 1 req/s：详情和角色之间歇一下
            vndb::polite_pause();
            let chars = vndb::fetch_characters(id, &Default::default()).unwrap_or_default();
            Ok((candidate, chars))
        }
        "bangumi" => bangumi::detail(id),
        "ymgal" => ymgal::detail(id),
        other => Err(format!("未知的信息源：{other}")),
    }
}

/* ------------------------------------------------------------ 标题匹配 */

/// 标题里这些词是"本地化/发布形态"的装饰，不是作品名的一部分。
///
/// 扫描出来的文件夹名经常是「王様恋愛 汉化版」「Summer Pockets 完整汉化」，
/// 直接和 VNDB 的原文标题比会全部落空。剥掉这些尾巴再比，命中率高一大截。
const TITLE_NOISE: &[&str] = &[
    "中文版",
    "汉化版",
    "完全汉化",
    "完整汉化",
    "汉化",
    "中文",
    "免安装",
    "绿色版",
    "硬盘版",
    "破解版",
    "未删减",
    "无修",
    "更新版",
];

/// 把标题归一化成"只留字母、数字、假名和汉字"的串。
///
/// 做四件事：
/// 1. 去掉所有括号内容（`【】（）()[]` 里常是版本、汉化组、广告）；
/// 2. 去掉空白与标点；
/// 3. 去掉 [`TITLE_NOISE`] 里的发布形态词；
/// 4. 统一小写、全角转半角（`Ｒｅ：` → `re:` → `re`）。
///
/// 注意：**不做**繁简转换 —— 那需要引入一张表，而"繁简不同"的标题在三个源里
/// 本来就各自返回各自的写法，靠人工兜底比靠错误的转换更安全。
pub fn normalize_title(input: &str) -> String {
    // 全角 → 半角（ASCII 可见区间），并把各种引号/破折号当标点处理
    let mut half = String::with_capacity(input.len());
    for ch in input.chars() {
        let code = ch as u32;
        if (0xFF01..=0xFF5E).contains(&code) {
            half.push(char::from_u32(code - 0xFEE0).unwrap_or(ch));
        } else if ch == '\u{3000}' {
            half.push(' ');
        } else {
            half.push(ch);
        }
    }

    // 去掉括号块（成对出现的，最多一层；标题里的括号几乎不嵌套）
    let mut no_brackets = String::with_capacity(half.len());
    let mut depth = 0i32;
    for ch in half.chars() {
        match ch {
            '【' | '（' | '(' | '[' | '［' | '〔' | '〈' | '《' => {
                depth += 1;
                continue;
            }
            '】' | '）' | ')' | ']' | '］' | '〕' | '〉' | '》' => {
                depth = (depth - 1).max(0);
                continue;
            }
            _ => {}
        }
        if depth == 0 {
            no_brackets.push(ch);
        }
    }

    // 只保留字母/数字/假名/汉字 —— 标点、空白、符号全部丢掉
    let mut kept: String = no_brackets
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();

    // 剥发布形态词（可能出现在任意位置，逐个删）
    for word in TITLE_NOISE {
        let w: String = word.chars().flat_map(|c| c.to_lowercase()).collect();
        while let Some(pos) = kept.find(&w) {
            kept.replace_range(pos..pos + w.len(), "");
        }
    }

    kept
}

/// 查询串和候选条目的相似度，0.0 ~ 1.0。
///
/// 1.0 = 归一化后与候选的**主标题 / 原文名 / 中文名**任意一个完全相等 —— 只有这种才敢自动写库。
/// 0.6~0.99 = 包含关系（短串至少 4 个字、且占长串 70% 以上），只用来给用户"最接近的是这个"提示。
pub fn title_score(query: &str, candidate: &Candidate) -> f64 {
    let q = normalize_title(query);
    if q.chars().count() < 2 {
        return 0.0;
    }

    let mut best: f64 = 0.0;
    for field in [&candidate.title, &candidate.alt_title, &candidate.title_cn] {
        let n = normalize_title(field);
        if n.is_empty() {
            continue;
        }
        if n == q {
            return 1.0;
        }

        let (short, long) = if n.chars().count() <= q.chars().count() {
            (&n, &q)
        } else {
            (&q, &n)
        };
        let short_len = short.chars().count();
        let long_len = long.chars().count();
        if short_len >= 4 && long.contains(short.as_str()) && long_len > 0 {
            let ratio = short_len as f64 / long_len as f64;
            // 0.6 是"提示"的门槛，不是"写入"的门槛：自动写入只认 1.0
            if ratio >= 0.6 {
                best = best.max(0.6 + 0.3 * ratio);
            }
        }
    }
    best
}

#[cfg(test)]
mod match_tests {
    use super::*;

    fn cand(title: &str, alt: &str, cn: &str) -> Candidate {
        Candidate {
            source: "vndb".to_string(),
            id: "v1".to_string(),
            title: title.to_string(),
            title_cn: cn.to_string(),
            alt_title: alt.to_string(),
            developer: String::new(),
            released: String::new(),
            rating: 0.0,
            description: String::new(),
            description_cn: String::new(),
            image_url: None,
        }
    }

    #[test]
    fn normalize_strips_version_noise_and_brackets() {
        assert_eq!(normalize_title("王様恋愛"), "王様恋愛");
        assert_eq!(normalize_title("王様恋愛 汉化版"), "王様恋愛");
        assert_eq!(normalize_title("【汉化】王様恋愛（完整汉化）"), "王様恋愛");
        assert_eq!(normalize_title("Summer Pockets - 完整汉化"), "summerpockets");
        assert_eq!(normalize_title("Ｒｅ：LieF"), "relief");
    }

    #[test]
    fn exact_title_wins_over_other_fields() {
        let c = cand("Ousama Ren'ai", "王様恋愛", "国王恋爱");
        assert_eq!(title_score("王様恋愛", &c), 1.0);
        assert_eq!(title_score("王様恋愛 汉化版", &c), 1.0);
        assert_eq!(title_score("国王恋爱", &c), 1.0);
    }

    #[test]
    fn partial_match_is_below_threshold() {
        let c = cand("Ousama Ren'ai", "王様恋愛", "");
        let s = title_score("王様恋愛外传", &c);
        assert!(s > 0.0 && s < 1.0, "部分包含不能算命中：{s}");
    }

    #[test]
    fn unrelated_title_scores_zero() {
        let c = cand("Summer Pockets", "サマーポケッツ", "夏日口袋");
        assert_eq!(title_score("王様恋愛", &c), 0.0);
    }

    #[test]
    fn very_short_query_never_matches() {
        let c = cand("AB", "AB", "AB");
        assert_eq!(title_score("A", &c), 0.0);
    }
}

