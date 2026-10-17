//! 数据访问层。
//!
//! 命令层（`commands.rs`）只负责拿 `State<Db>` 和做错误转换，
//! 真正的 SQL 写在这里 —— 这样可以用 `cargo test` 直接验证，不需要拉起窗口。

use crate::models::{Game, GamePatch, GAME_COLUMNS};
use rusqlite::types::Value;
use rusqlite::{params, Connection};

pub fn list_all(conn: &Connection) -> rusqlite::Result<Vec<Game>> {
    let sql = format!(
        "SELECT {GAME_COLUMNS} FROM games \
         WHERE ignored = 0 \
         ORDER BY first_seen_at DESC, title_cn ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], Game::from_row)?;

    let mut games = Vec::new();
    for row in rows {
        games.push(row?);
    }
    Ok(games)
}

/// 按 id 取单条。
pub fn get_one(conn: &Connection, id: &str) -> rusqlite::Result<Option<Game>> {
    let sql = format!("SELECT {GAME_COLUMNS} FROM games WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map([id], Game::from_row)?;
    match rows.next() {
        Some(Ok(g)) => Ok(Some(g)),
        Some(Err(e)) => Err(e),
        None => Ok(None),
    }
}

/// 数据库列名 → `locked_fields` 里记录的名字。
///
/// 名字沿用旧库已有的 camelCase 习惯（迁移过来的数据里就是 `titleCn` / `coverUrl`），
/// 不能换成 snake_case，否则同一份数据里会出现两种写法。
const LOCK_NAMES: &[(&str, &str)] = &[
    ("title", "title"),
    ("title_cn", "titleCn"),
    ("original_title", "originalTitle"),
    ("developer", "developer"),
    ("released", "released"),
    ("engine", "engine"),
    ("platform", "platform"),
    ("tags", "tags"),
    ("rating", "rating"),
    ("rating_source", "ratingSource"),
    ("description", "description"),
    ("cn_description", "cnDescription"),
    ("cover_url", "coverUrl"),
    ("launch_exe", "launchExe"),
    ("completed", "completed"),
    ("vndb_id", "vndbId"),
];

fn lock_name(col: &str) -> &'static str {
    LOCK_NAMES
        .iter()
        .find(|(c, _)| *c == col)
        .map(|(_, n)| *n)
        .unwrap_or("unknown")
}

fn push_text(cols: &mut Vec<&'static str>, vals: &mut Vec<Value>, col: &'static str, v: &Option<String>) {
    if let Some(s) = v {
        cols.push(col);
        vals.push(Value::Text(s.clone()));
    }
}

/// 按补丁更新一条游戏，返回**更新后从库里重新读出来的那一行**。
///
/// 几个刻意的设计：
/// - `cols` 只从上面的常量白名单里取，SQL 里的列名永远不会来自外部输入；
/// - 补丁里没出现的列不进 `SET`，没被碰过的字段（比如 `first_seen_at`）不会被覆盖；
/// - `locked_changed = true` 时，把本次改动的字段并进 `locked_fields`（去重），
///   这就是界面上那个小锁图标的来源 —— 旧库用 7 张 Map 才能表达的东西，现在是一列。
pub fn update_game(
    conn: &Connection,
    id: &str,
    patch: &GamePatch,
    lock_changed: bool,
) -> Result<Game, String> {
    patch.validate()?;

    let mut cols: Vec<&'static str> = Vec::new();
    let mut vals: Vec<Value> = Vec::new();

    push_text(&mut cols, &mut vals, "title", &patch.title);
    push_text(&mut cols, &mut vals, "title_cn", &patch.title_cn);
    push_text(&mut cols, &mut vals, "original_title", &patch.original_title);
    push_text(&mut cols, &mut vals, "developer", &patch.developer);
    push_text(&mut cols, &mut vals, "released", &patch.released);
    push_text(&mut cols, &mut vals, "engine", &patch.engine);
    push_text(&mut cols, &mut vals, "platform", &patch.platform);
    push_text(&mut cols, &mut vals, "rating_source", &patch.rating_source);
    push_text(&mut cols, &mut vals, "description", &patch.description);
    push_text(&mut cols, &mut vals, "cn_description", &patch.cn_description);
    push_text(&mut cols, &mut vals, "launch_exe", &patch.launch_exe);
    push_text(&mut cols, &mut vals, "vndb_id", &patch.vndb_id);

    if let Some(url) = &patch.cover_url {
        cols.push("cover_url");
        vals.push(Value::Text(url.clone()));
    }
    if let Some(tags) = &patch.tags {
        cols.push("tags");
        vals.push(Value::Text(
            serde_json::to_string(tags).map_err(|e| format!("标签序列化失败：{e}"))?,
        ));
    }
    if let Some(r) = patch.rating {
        cols.push("rating");
        vals.push(Value::Real(r));
    }
    if let Some(c) = patch.completed {
        cols.push("completed");
        vals.push(Value::Integer(if c { 1 } else { 0 }));
    }

    // 完全没有改动：不写库，把当前行原样返回，交给前端提示"没有检测到改动"
    if cols.is_empty() {
        return get_one(conn, id)
            .map_err(|e| format!("数据库操作失败：{e}"))?
            .ok_or_else(|| format!("找不到 id 为 {id} 的游戏"));
    }

    let mut assignments: Vec<String> = cols.iter().map(|c| format!("{c} = ?")).collect();

    if lock_changed {
        let current = get_one(conn, id)
            .map_err(|e| format!("数据库操作失败：{e}"))?
            .ok_or_else(|| format!("找不到 id 为 {id} 的游戏"))?;

        let mut merged = current.locked_fields;
        for c in &cols {
            let name = lock_name(c).to_string();
            if !merged.contains(&name) {
                merged.push(name);
            }
        }
        assignments.push("locked_fields = ?".to_string());
        vals.push(Value::Text(
            serde_json::to_string(&merged).map_err(|e| format!("锁定字段序列化失败：{e}"))?,
        ));
    }

    // 时间戳交给 SQLite，避免为格式化 ISO 时间引日期库
    assignments.push("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')".to_string());

    let sql = format!(
        "UPDATE games SET {} WHERE id = ?",
        assignments.join(", ")
    );
    vals.push(Value::Text(id.to_string()));

    let affected = conn
        .execute(&sql, rusqlite::params_from_iter(vals))
        .map_err(|e| format!("数据库操作失败：{e}"))?;

    if affected == 0 {
        return Err(format!("找不到 id 为 {id} 的游戏"));
    }

    get_one(conn, id)
        .map_err(|e| format!("数据库操作失败：{e}"))?
        .ok_or_else(|| format!("更新后读不回来（id={id}）"))
}

/// 演示数据用的固定 id（与 insert_demo_games 里一致）。
/// 迁移前必须先清掉它们，否则真实库里会混进 4 条假游戏。
pub const DEMO_IDS: [&str; 4] = ["a1b2c3d4", "9c874865", "3f0d5a71", "b1e9f420"];

/// 删除演示数据。返回删除行数。
pub fn purge_demo_games(conn: &Connection) -> rusqlite::Result<usize> {
    let placeholders = DEMO_IDS.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM games WHERE id IN ({placeholders})");
    conn.execute(&sql, rusqlite::params_from_iter(DEMO_IDS.iter()))
}

pub fn count_all(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
}

/// 写入 4 条演示数据（INSERT OR IGNORE：重复调用不会重复插入）。
/// 返回写入后的总行数。
pub fn insert_demo_games(conn: &Connection) -> rusqlite::Result<i64> {
    // (id/path_hash, 英文名, 中文名, 原名, 会社, 发售日, 引擎, 平台, 标签, 评分, 评分源, 主程序, 目录, 文件数, 已通关)
    #[allow(clippy::type_complexity)]
    let rows: [(&str, &str, &str, &str, &str, &str, &str, &str, &str, f64, &str, &str, &str, i64, i64); 4] = [
        (
            "a1b2c3d4", "Hoshizora no Memory", "星光记忆", "星空のメモリア",
            "Starlight Works", "2021-07-30", "kirikiri", "windows",
            r#"["纯爱","校园","治愈"]"#, 8.6, "vndb", "Hoshimemo.exe",
            r"E:\galgame\StarlightWorks\星光记忆", 812, 1,
        ),
        (
            "9c874865", "Summer Pockets", "夏日口袋", "サマーポケッツ",
            "Key", "2018-06-29", "siglus", "windows",
            r#"["夏日","亲情","催泪"]"#, 9.1, "vndb", "SummerPockets.exe",
            r"E:\galgame\Key\夏日口袋", 1204, 0,
        ),
        (
            "3f0d5a71", "Sakura no Uta", "樱之诗", "サクラノ詩",
            "枕", "2015-10-23", "kirikiri", "windows",
            r#"["艺术","哲学","催泪"]"#, 9.3, "vndb", "SakuraNoUta.exe",
            r"E:\galgame\枕\樱之诗", 963, 0,
        ),
        (
            "b1e9f420", "Senren * Banka", "千恋万花", "千恋＊万花",
            "ゆずソフト", "2016-07-29", "kirikiri", "windows",
            r#"["和风","巫女","甜蜜"]"#, 8.4, "bangumi", "SenrenBanka.exe",
            r"E:\galgame\yuzusoft\千恋万花", 735, 0,
        ),
    ];

    // 时间戳交给 SQLite，避免为了格式化 ISO 时间引一个日期库
    let sql = "INSERT OR IGNORE INTO games (
            id, path_hash, title, title_cn, original_title, developer, released,
            engine, platform, tags, rating, rating_source, launch_exe,
            folder_path, folder_name, file_count, match_score, completed,
            root_path, saved_at, added_at, updated_at, first_seen_at
        ) VALUES (
            ?1, ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, 15.0, ?16,
            'E:\\galgame',
            strftime('%Y-%m-%dT%H:%M:%SZ','now'),
            strftime('%Y-%m-%dT%H:%M:%SZ','now'),
            strftime('%Y-%m-%dT%H:%M:%SZ','now'),
            strftime('%Y-%m-%dT%H:%M:%SZ','now')
        )";

    let mut stmt = conn.prepare(sql)?;
    for r in rows.iter() {
        stmt.execute(params![
            r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11, r.12, r.2, r.13, r.14,
        ])?;
    }

    count_all(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_at;
    use crate::models::GamePatch;

    fn temp_db(tag: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join("moeshelf-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tag}.db"));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        let conn = open_at(&path).expect("建库失败");
        (path, conn)
    }

    #[test]
    fn schema_has_four_tables_and_version() {
        let (_, conn) = temp_db("schema");
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
                 ('games','playtime','scan_roots','game_sources')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 4, "四张核心表必须都存在");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn insert_demo_then_list_roundtrip() {
        let (_, conn) = temp_db("roundtrip");
        assert_eq!(list_all(&conn).unwrap().len(), 0, "新库应该是空的");

        let total = insert_demo_games(&conn).expect("插入失败");
        assert_eq!(total, 4);

        // 重复调用不应该产生重复行
        let again = insert_demo_games(&conn).expect("二次插入失败");
        assert_eq!(again, 4, "INSERT OR IGNORE 必须幂等");

        let games = list_all(&conn).unwrap();
        assert_eq!(games.len(), 4);

        let sakura = games.iter().find(|g| g.title_cn == "樱之诗").expect("没查到樱之诗");
        assert_eq!(sakura.engine, "kirikiri");
        assert_eq!(sakura.developer, "枕");
        assert_eq!(sakura.tags, vec!["艺术", "哲学", "催泪"]);
        assert!(sakura.completed == false);
        assert!(sakura.first_seen_at.ends_with('Z'), "时间戳应是 ISO 格式");
    }

    /* ------------------------------------------------------------ update_game */

    fn patch(f: impl FnOnce(&mut GamePatch)) -> GamePatch {
        let mut p = GamePatch::default();
        f(&mut p);
        p
    }

    #[test]
    fn update_writes_only_changed_columns_and_merges_locks() {
        let (_, conn) = temp_db("update");
        insert_demo_games(&conn).unwrap();
        let id = "a1b2c3d4";

        let p = patch(|p| {
            p.title_cn = Some("星光记忆（改）".to_string());
            p.rating = Some(9.2);
        });
        let updated = update_game(&conn, id, &p, true).expect("更新失败");

        // 改了的字段生效
        assert_eq!(updated.title_cn, "星光记忆（改）");
        assert_eq!(updated.rating, 9.2);
        // 没出现在补丁里的字段原样保留
        assert_eq!(updated.developer, "Starlight Works");
        assert_eq!(updated.tags, vec!["纯爱", "校园", "治愈"]);
        assert_eq!(updated.launch_exe, "Hoshimemo.exe");
        // 锁定名单里记的是 camelCase，且只记改过的
        assert!(updated.locked_fields.contains(&"titleCn".to_string()));
        assert!(updated.locked_fields.contains(&"rating".to_string()));
        assert!(!updated.locked_fields.contains(&"developer".to_string()));
    }

    #[test]
    fn update_does_not_duplicate_lock_entries() {
        let (_, conn) = temp_db("lock-dedup");
        insert_demo_games(&conn).unwrap();
        let id = "a1b2c3d4";

        let p = patch(|p| p.developer = Some("Yuzusoft".to_string()));
        update_game(&conn, id, &p, true).unwrap();
        let again = update_game(&conn, id, &p, true).unwrap();

        let hits = again
            .locked_fields
            .iter()
            .filter(|f| f.as_str() == "developer")
            .count();
        assert_eq!(hits, 1, "同一个字段不能重复进锁定名单");
    }

    #[test]
    fn update_without_lock_leaves_locked_fields_untouched() {
        let (_, conn) = temp_db("no-lock");
        insert_demo_games(&conn).unwrap();
        let id = "a1b2c3d4";

        let p = patch(|p| p.developer = Some("Key".to_string()));
        let updated = update_game(&conn, id, &p, false).unwrap();
        assert_eq!(updated.developer, "Key");
        assert!(updated.locked_fields.is_empty(), "不锁定就不该写 locked_fields");
    }

    #[test]
    fn update_survives_reopen() {
        // 这条是"编辑 → 保存 → 关掉应用 → 重开还在"的自动化版本
        let (path, conn) = temp_db("persist");
        insert_demo_games(&conn).unwrap();

        let p = patch(|p| p.title_cn = Some("持久化验证".to_string()));
        update_game(&conn, "a1b2c3d4", &p, true).unwrap();
        drop(conn);

        let conn2 = open_at(&path).expect("重开失败");
        let row = get_one(&conn2, "a1b2c3d4").unwrap().expect("行丢了");
        assert_eq!(row.title_cn, "持久化验证");
        assert!(row.locked_fields.contains(&"titleCn".to_string()));
    }

    #[test]
    fn update_rejects_bad_values() {
        let (_, conn) = temp_db("validate");
        insert_demo_games(&conn).unwrap();

        let bad_rating = patch(|p| p.rating = Some(42.0));
        assert!(update_game(&conn, "a1b2c3d4", &bad_rating, true).is_err());

        let bad_date = patch(|p| p.released = Some("2016/07/29".to_string()));
        assert!(update_game(&conn, "a1b2c3d4", &bad_date, true).is_err());

        let empty_title = patch(|p| p.title_cn = Some("   ".to_string()));
        assert!(update_game(&conn, "a1b2c3d4", &empty_title, true).is_err());

        // 校验失败不能污染数据
        let row = get_one(&conn, "a1b2c3d4").unwrap().unwrap();
        assert_eq!(row.title_cn, "星光记忆");
        assert_eq!(row.locked_fields.len(), 0);
    }

    #[test]
    fn update_missing_id_is_error() {
        let (_, conn) = temp_db("missing");
        let p = patch(|p| p.title_cn = Some("x".to_string()));
        assert!(update_game(&conn, "deadbeef", &p, true).is_err());
    }
}
