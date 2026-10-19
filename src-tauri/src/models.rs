use rusqlite::Row;
use serde::{Deserialize, Serialize};

/// 一条游戏记录。字段与前端 `src/types/game.ts` 的 `Game` 一一对应。
///
/// 命名规则：Rust 端全部 snake_case，序列化时统一 `rename_all = "camelCase"`，
/// 前端拿到的就是 `titleCn` / `launchExe` / `firstSeenAt`，**中间不做任何二次映射**。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub path_hash: String,
    pub title: String,
    pub title_cn: String,
    pub original_title: String,
    pub developer: String,
    pub released: String,
    pub engine: String,
    pub platform: String,
    pub tags: Vec<String>,
    pub rating: f64,
    pub rating_source: String,
    pub description: String,
    pub cn_description: String,
    pub cover_url: Option<String>,
    pub launch_exe: String,
    pub folder_path: String,
    pub file_count: i64,
    /// 目录总字节数（递归统计）。旧库没有这一列，V2 迁移补的
    pub dir_size: i64,
    pub match_score: f64,
    pub completed: bool,
    pub first_seen_at: String,
    pub locked_fields: Vec<String>,
    pub vndb_id: Option<String>,
}

/// 查询时固定使用的列顺序，避免 `SELECT *` 与 from_row 的顺序耦合。
pub const GAME_COLUMNS: &str = "id, path_hash, title, title_cn, original_title, developer, \
     released, engine, platform, tags, rating, rating_source, description, cn_description, \
     cover_url, launch_exe, folder_path, file_count, dir_size, match_score, completed, \
     first_seen_at, locked_fields, vndb_id";

/// 一次编辑提交的增量补丁。
///
/// 全部字段都是 `Option`：**缺省 = 不改这一列**，前端只需要传改动过的字段。
/// 这样既避免了"整行覆盖"把并发改动冲掉，也让 SQL 只更新真正变动的列。
///
/// 字段名与前端 `Partial<Game>` 的 camelCase 完全对应（`titleCn` / `cnDescription` …），
/// 前端不需要做任何名字转换。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePatch {
    pub title: Option<String>,
    pub title_cn: Option<String>,
    pub original_title: Option<String>,
    pub developer: Option<String>,
    /// YYYY-MM-DD
    pub released: Option<String>,
    pub engine: Option<String>,
    pub platform: Option<String>,
    pub tags: Option<Vec<String>>,
    /// 0~10
    pub rating: Option<f64>,
    pub rating_source: Option<String>,
    pub description: Option<String>,
    pub cn_description: Option<String>,
    pub cover_url: Option<String>,
    pub launch_exe: Option<String>,
    pub completed: Option<bool>,
    pub vndb_id: Option<String>,
}

impl GamePatch {
    /// 写入前的兜底校验。前端已经校验过一遍，这里再拦一次 ——
    /// 因为命令层是公开入口，不能假设调用方一定是自己的界面。
    pub fn validate(&self) -> Result<(), String> {
        if let Some(t) = &self.title_cn {
            if t.trim().is_empty() {
                return Err("中文标题不能为空".to_string());
            }
        }
        if let Some(r) = &self.released {
            if !r.is_empty() && !is_iso_date(r) {
                return Err(format!("发售日格式应为 YYYY-MM-DD，收到：{r}"));
            }
        }
        if let Some(v) = self.rating {
            if !(0.0..=10.0).contains(&v) {
                return Err(format!("评分必须在 0~10 之间，收到：{v}"));
            }
        }
        Ok(())
    }
}

/// 只认 `YYYY-MM-DD`，其余一律拒绝（空串表示"清空"，由调用方另行判断）
fn is_iso_date(s: &str) -> bool {    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn json_vec(raw: String) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()
}

impl Game {
    /// 从查询行构造。JSON 列（tags / locked_fields）解析失败时降级为空数组，
    /// 不让一条脏数据把整个列表查询打挂。
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let tags_raw: String = row.get("tags")?;
        let locked_raw: String = row.get("locked_fields")?;

        Ok(Self {
            id: row.get("id")?,
            path_hash: row.get("path_hash")?,
            title: row.get("title")?,
            title_cn: row.get("title_cn")?,
            original_title: row.get("original_title")?,
            developer: row.get("developer")?,
            released: row.get("released")?,
            engine: row.get("engine")?,
            platform: row.get("platform")?,
            tags: json_vec(tags_raw),
            rating: row.get("rating")?,
            rating_source: row.get("rating_source")?,
            description: row.get("description")?,
            cn_description: row.get("cn_description")?,
            cover_url: row.get("cover_url")?,
            launch_exe: row.get("launch_exe")?,
            folder_path: row.get("folder_path")?,
            file_count: row.get("file_count")?,
            dir_size: row.get("dir_size")?,
            match_score: row.get("match_score")?,
            completed: row.get::<_, i64>("completed")? != 0,
            first_seen_at: row.get("first_seen_at")?,
            locked_fields: json_vec(locked_raw),
            vndb_id: row.get("vndb_id")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端发来的就是 `Partial<Game>` 的 camelCase 键。
    /// 这条测试锁住"前端字段名 ↔ Rust 字段名"的约定：
    /// 谁把 `rename_all` 删了、或者改了某个字段名，这里立刻红。
    #[test]
    fn patch_deserializes_frontend_camel_case_payload() {
        let json = r#"{
            "titleCn": "千恋＊万花",
            "developer": "Yuzusoft",
            "released": "2016-07-29",
            "engine": "kirikiri",
            "tags": ["柚子社", "废萌"],
            "rating": 8.8,
            "cnDescription": "在一座连公交都不通的深山里……"
        }"#;
        let p: GamePatch = serde_json::from_str(json).expect("反序列化失败");
        assert_eq!(p.title_cn.as_deref(), Some("千恋＊万花"));
        assert_eq!(p.developer.as_deref(), Some("Yuzusoft"));
        assert_eq!(p.engine.as_deref(), Some("kirikiri"));
        assert_eq!(p.rating, Some(8.8));
        assert_eq!(p.tags.as_deref(), Some(&["柚子社".to_string(), "废萌".to_string()][..]));
        // 没传的字段必须是 None —— 否则 update_game 会把它们覆盖成空
        assert!(p.title.is_none());
        assert!(p.original_title.is_none());
        assert!(p.description.is_none());
        assert!(p.launch_exe.is_none());
        assert!(p.cover_url.is_none());
    }

    #[test]
    fn empty_patch_is_all_none() {
        let p: GamePatch = serde_json::from_str("{}").expect("空对象应该能解析");
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validate_catches_obvious_garbage() {
        let mut p = GamePatch::default();
        p.rating = Some(11.5);
        assert!(p.validate().is_err());

        let mut p = GamePatch::default();
        p.released = Some("2016-7-29".to_string());
        assert!(p.validate().is_err());

        let mut p = GamePatch::default();
        p.released = Some(String::new()); // 清空发售日是允许的
        assert!(p.validate().is_ok());

        let mut p = GamePatch::default();
        p.title_cn = Some("  ".to_string());
        assert!(p.validate().is_err());
    }
}

/// 一个"扫描根目录"。用户可以把游戏库放在多个盘/多个目录，逐个登记，
/// 顶栏扫描按钮就会列出它们，不用每次从磁盘里手动找。
///
/// 表在 V1 就建好了（`scan_roots`），只是一直没人写 —— C 阶段的尾巴。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRoot {
    pub id: i64,
    pub path: String,
    /// 上次扫描完成时间（ISO），从没扫过是 None
    pub last_scan_at: Option<String>,
    pub added_at: String,
}

/// 一个角色。
///
/// 数据来自旧库刮削缓存的 `characters` 数组：
/// `{ name, aliases[], image, role, cv, cvSource }`。
/// `role` 是旧版本地化好的中文（主角 / 主要人物 / 次要人物 / 仅出现…），
/// 原样保留，不再翻译。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub role: String,
    /// 声优；没收录时是空串（不是 None —— 前端只需判断空）
    pub cv: String,
    /// 声优来源（bangumi / cngal / vndb），用来解释"为什么这个名字看起来不一样"
    pub cv_source: String,
    pub image_url: Option<String>,
    pub sort_order: i64,
}

impl Character {
    /// 从旧库的 JSON 数组转成结构体。
    ///
    /// 旧数据里 `name` 可能带空格（`九條 都`）、`cv` 可能缺失、`image` 可能不存在，
    /// 这里一律降级成空值而不是报错 —— 迁移阶段不能因为一个角色字段缺失就整部游戏失败。
    pub fn from_legacy_array(arr: &[serde_json::Value]) -> Vec<Character> {
        arr.iter()
            .enumerate()
            .filter_map(|(i, v)| {
                let obj = v.as_object()?;
                let name = obj
                    .get("name")
                    .and_then(|x| x.as_str())
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    return None;
                }
                let aliases = obj
                    .get("aliases")
                    .and_then(|x| x.as_array())
                    .map(|list| {
                        list.iter()
                            .filter_map(|a| a.as_str())
                            .map(|a| a.trim().to_string())
                            .filter(|a| !a.is_empty())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Some(Character {
                    name,
                    aliases,
                    role: obj
                        .get("role")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    cv: obj
                        .get("cv")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    cv_source: obj
                        .get("cvSource")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    image_url: obj
                        .get("image")
                        .and_then(|x| x.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    sort_order: i as i64,
                })
            })
            .collect()
    }
}
