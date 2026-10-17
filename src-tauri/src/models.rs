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
    pub match_score: f64,
    pub completed: bool,
    pub first_seen_at: String,
    pub locked_fields: Vec<String>,
    pub vndb_id: Option<String>,
}

/// 查询时固定使用的列顺序，避免 `SELECT *` 与 from_row 的顺序耦合。
pub const GAME_COLUMNS: &str = "id, path_hash, title, title_cn, original_title, developer, \
     released, engine, platform, tags, rating, rating_source, description, cn_description, \
     cover_url, launch_exe, folder_path, file_count, match_score, completed, first_seen_at, \
     locked_fields, vndb_id";

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
            match_score: row.get("match_score")?,
            completed: row.get::<_, i64>("completed")? != 0,
            first_seen_at: row.get("first_seen_at")?,
            locked_fields: json_vec(locked_raw),
            vndb_id: row.get("vndb_id")?,
        })
    }
}
