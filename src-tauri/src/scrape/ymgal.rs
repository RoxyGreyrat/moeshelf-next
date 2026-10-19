//! 月幕 Galgame（ymgal.games）数据源。
//!
//! 国内的中文 Galgame 资料站，优点是**有官方中文简介**（中文简介比 VNDB 的英文有用得多），
//! 也有中文名、发售日、会社、封面。
//!
//! 鉴权：`/oauth/token` 走 client_credentials，拿到 access_token 后带
//! `Authorization: Bearer` + `version: 1` 两个头。token 有效期约 3500s，缓存起来复用。
//!
//! 局限：接口不返回角色名单（只有 cid 数字），所以这个源只补资料，不补角色。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::Candidate;

const BASE: &str = "https://www.ymgal.games";

/// token 缓存：(token, 过期时间)。全局只存一份，避免每搜一次都换 token。
static TOKEN: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn token() -> Result<String, String> {
    if let Ok(guard) = TOKEN.lock() {
        if let Some((t, exp)) = guard.as_ref() {
            if Instant::now() < *exp {
                return Ok(t.clone());
            }
        }
    }

    let resp = super::client()?
        .post(format!("{BASE}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(
            "grant_type=client_credentials&client_id=ymgal&client_secret=luna0327&scope=public",
        )
        .send()
        .map_err(|e| format!("月幕鉴权失败：{e}"))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取鉴权响应失败：{e}"))?;
    if !status.is_success() {
        return Err(format!("月幕鉴权返回 HTTP {status}"));
    }

    let v: Value = serde_json::from_str(&text).map_err(|e| format!("解析鉴权响应失败：{e}"))?;
    let token = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "月幕鉴权没有返回 access_token".to_string())?
        .to_string();

    let ttl = v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(2400);
    if let Ok(mut guard) = TOKEN.lock() {
        // 提前 60 秒过期，避免边界上拿到刚失效的 token
        *guard = Some((
            token.clone(),
            Instant::now() + Duration::from_secs(ttl.saturating_sub(60)),
        ));
    }

    Ok(token)
}

fn auth_headers() -> Result<Vec<(&'static str, String)>, String> {
    Ok(vec![
        ("Authorization", format!("Bearer {}", token()?)),
        // 这个头是必须的：不带会返回 404 "Not found or incorrect version."
        ("version", "1".to_string()),
    ])
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

/// 搜索候选
pub fn search(query: &str, limit: u32) -> Result<Vec<Candidate>, String> {
    let url = format!(
        "{BASE}/open/archive/search-game?keyword={}&pageNum=1&pageSize={}&mode=list",
        urlencode(query),
        limit.clamp(1, 25)
    );

    let v = super::get_json(&url, &auth_headers()?)?;

    if v.get("success").and_then(|x| x.as_bool()) == Some(false) {
        return Err(format!(
            "月幕返回失败：{}",
            s(&v, "msg").chars().take(160).collect::<String>()
        ));
    }

    let list = v
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(list.iter().map(candidate_from_search).collect())
}

fn candidate_from_search(v: &Value) -> Candidate {
    let name = s(v, "name");
    let cn = s(v, "chineseName");
    let score: f64 = s(v, "score").parse().unwrap_or(0.0);

    Candidate {
        source: "ymgal".to_string(),
        id: s(v, "id"),
        // 月幕没有单独的罗马字字段，中文名和原名分开存
        title: name.clone(),
        title_cn: cn,
        alt_title: name,
        developer: s(v, "orgName"),
        released: s(v, "releaseDate"),
        // 月幕的 score 常见为 0（没评分），原样保留，应用时 0 不覆盖
        rating: score,
        description: String::new(),
        description_cn: String::new(),
        image_url: Some(s(v, "mainImg")).filter(|u| !u.is_empty()),
    }
}

/// 取详情：中文简介、封面、发售日
pub fn detail(gid: &str) -> Result<(Candidate, Vec<crate::models::Character>), String> {
    let url = format!("{BASE}/open/archive?gid={gid}");
    let v = super::get_json(&url, &auth_headers()?)?;

    let game = v
        .get("data")
        .and_then(|d| d.get("game"))
        .cloned()
        .ok_or_else(|| format!("月幕上找不到 {gid}"))?;

    let name = s(&game, "name");
    let cn = s(&game, "chineseName");

    // 英文名在 extensionName[0].name，用它当"罗马字标题"
    let english = game
        .get("extensionName")
        .and_then(|e| e.as_array())
        .and_then(|list| list.first())
        .map(|x| s(x, "name"))
        .unwrap_or_default();

    // 详情接口不返回会社名，靠再搜一次按 id 对齐拿（只多一个请求）
    let developer = search(&name, 10)
        .ok()
        .and_then(|list| list.into_iter().find(|c| c.id == gid))
        .map(|c| c.developer)
        .unwrap_or_default();

    let candidate = Candidate {
        source: "ymgal".to_string(),
        id: gid.to_string(),
        title: if english.is_empty() { name.clone() } else { english },
        title_cn: cn,
        alt_title: name,
        developer,
        released: s(&game, "releaseDate"),
        rating: 0.0,
        description: String::new(),
        // 月幕的简介是中文，直接进 cn_description
        description_cn: s(&game, "introduction"),
        image_url: Some(s(&game, "mainImg")).filter(|u| !u.is_empty()),
    };

    Ok((candidate, Vec::new()))
}

/// 最小可用的 URL 编码（只处理非 ASCII 与常见保留字符）
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_non_ascii() {
        assert_eq!(urlencode("千恋"), "%E5%8D%83%E6%81%8B");
        assert_eq!(urlencode("Senren Banka"), "Senren%20Banka");
    }

    #[test]
    fn parses_search_item() {
        let v = serde_json::json!({
            "id": 22374,
            "name": "千恋＊万花",
            "chineseName": "千恋万花",
            "mainImg": "https://cdn.ymgal.games/x.webp",
            "releaseDate": "2016-07-29",
            "orgName": "ゆずソフト",
            "score": "0"
        });
        let c = candidate_from_search(&v);
        assert_eq!(c.id, "22374");
        assert_eq!(c.title_cn, "千恋万花");
        assert_eq!(c.developer, "ゆずソフト");
        assert_eq!(c.rating, 0.0);
    }
}
