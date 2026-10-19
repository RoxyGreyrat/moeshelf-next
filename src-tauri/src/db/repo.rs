//! 数据访问层。
//!
//! 命令层（`commands.rs`）只负责拿 `State<Db>` 和做错误转换，
//! 真正的 SQL 写在这里 —— 这样可以用 `cargo test` 直接验证，不需要拉起窗口。

use crate::models::{Character, Game, GamePatch, GAME_COLUMNS};
use rusqlite::types::Value;
use rusqlite::Connection;

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

/// 演示数据用的固定 id（`insert_demo_games` 里用的就是它们）。
/// 迁移前必须先清掉它们，否则真实库里会混进 4 条假游戏。
pub const DEMO_IDS: [&str; 4] = ["a1b2c3d4", "9c874865", "3f0d5a71", "b1e9f420"];

/// 删除演示数据。返回删除行数。
pub fn purge_demo_games(conn: &Connection) -> rusqlite::Result<usize> {
    let placeholders = DEMO_IDS.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM games WHERE id IN ({placeholders})");
    conn.execute(&sql, rusqlite::params_from_iter(DEMO_IDS.iter()))
}

/// 全表行数。仅供测试断言使用。
#[cfg(test)]
pub fn count_all(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
}

/// 写入 4 条演示数据（INSERT OR IGNORE：重复调用不会重复插入）。
///
/// **只在测试里用**：B1 联调时它由界面上的临时按钮驱动，B3 打通真实数据后
/// 命令与按钮都已删除，现在它只是测试夹具（构造一批已知行）。
#[cfg(test)]
pub fn insert_demo_games(conn: &Connection) -> rusqlite::Result<i64> {
    use rusqlite::params;

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
        assert_eq!(version, crate::db::schema::SCHEMA_VERSION);

        // V2 加的那一列必须在
        let has_dir_size: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('games') WHERE name = 'dir_size'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_dir_size, 1, "V2 必须补出 dir_size 列");
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

/* ------------------------------------------------------------ 扫描结果写库 */

use crate::db::scanner::ScannedGame;
use rusqlite::OptionalExtension;

/// `INSERT ... ON CONFLICT` 的写法要点（改之前先读）：
///
/// - **保留人工数据**：`title_cn` / `tags` / `rating` / `cover_url` 这些列在冲突时**不覆盖**，
///   只在新行时填 `folder_name` 兜底。扫一次不能把刮削出来或人工改过的元数据冲掉。
/// - **`first_seen_at` 只在插入时写**，冲突时不动 —— 它是"入库时间"，不是"最后扫描时间"。
/// - **`launch_exe` / `engine` 受 `locked_fields` 保护**：
///   用 `instr(locked_fields, '"launchExe"')` 判断锁没锁，锁了就不动。
/// - **`engine` 只在原值是 `unknown` 时才被覆盖**，避免把迁移时指纹识别出来的结果
///   再扫成 unknown（扫描器的引擎判定更保守）。
const UPSERT_SCANNED_SQL: &str = r#"
INSERT INTO games (
    id, path_hash, title, title_cn, original_title, developer, released,
    engine, platform, tags, rating, rating_source, description, cn_description,
    cover_url, launch_exe, folder_path, folder_name, file_count, dir_size,
    match_score, completed, root_path, saved_at, added_at, updated_at,
    first_seen_at, locked_fields, field_sources, manual_match, ignored
) VALUES (
    ?1, ?1, '', ?2, '', '', '',
    ?3, 'windows', '[]', 0, '', '', '',
    NULL, ?4, ?5, ?6, ?7, ?8,
    ?9, 0, ?10, '',
    strftime('%Y-%m-%dT%H:%M:%SZ','now'),
    strftime('%Y-%m-%dT%H:%M:%SZ','now'),
    strftime('%Y-%m-%dT%H:%M:%SZ','now'),
    '[]', '{}', NULL, 0
)
ON CONFLICT(path_hash) DO UPDATE SET
    folder_path = excluded.folder_path,
    folder_name = excluded.folder_name,
    file_count  = excluded.file_count,
    dir_size    = excluded.dir_size,
    match_score = excluded.match_score,
    updated_at  = excluded.updated_at,
    title_cn    = CASE WHEN games.title_cn = '' THEN excluded.title_cn ELSE games.title_cn END,
    launch_exe  = CASE
        WHEN instr(games.locked_fields, '"launchExe"') > 0 THEN games.launch_exe
        WHEN excluded.launch_exe = '' THEN games.launch_exe
        ELSE excluded.launch_exe
    END,
    engine      = CASE
        WHEN instr(games.locked_fields, '"engine"') > 0 THEN games.engine
        WHEN games.engine = 'unknown' AND excluded.engine <> 'unknown' THEN excluded.engine
        ELSE games.engine
    END
"#;

/// 写入一条扫描结果。返回 `true` 表示新增，`false` 表示更新了已有行。
pub fn upsert_scanned(conn: &Connection, g: &ScannedGame) -> rusqlite::Result<bool> {
    let existed: bool = conn
        .query_row(
            "SELECT 1 FROM games WHERE path_hash = ?1",
            [&g.path_hash],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);

    let now_root = g
        .folder_path
        .rsplit_once(std::path::MAIN_SEPARATOR)
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default();

    conn.execute(
        UPSERT_SCANNED_SQL,
        rusqlite::params![
            g.path_hash,     // ?1
            g.folder_name,   // ?2
            g.engine,        // ?3
            g.launch_exe,    // ?4
            g.folder_path,   // ?5
            g.folder_name,   // ?6
            g.file_count,    // ?7
            g.dir_size as i64, // ?8
            g.match_score,   // ?9
            now_root,        // ?10
        ],
    )?;

    Ok(!existed)
}

/* ------------------------------------------------------------ 库维护：忽略 / 删除 */

/// 列出被忽略的条目（界面上在「设置 → 已忽略」里显示，可以恢复）。
pub fn list_ignored(conn: &Connection) -> rusqlite::Result<Vec<Game>> {
    let sql = format!(
        "SELECT {GAME_COLUMNS} FROM games \
         WHERE ignored = 1 \
         ORDER BY folder_path ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], Game::from_row)?;

    let mut games = Vec::new();
    for row in rows {
        games.push(row?);
    }
    Ok(games)
}

/// 把若干条标记为忽略 / 取消忽略。返回真正被改动的行数。
///
/// **忽略不是删除**：行留在库里，只是 `ignored = 1`，`list_games` 会把它过滤掉。
/// 这样做的好处是重扫时不用特殊处理：`upsert_scanned` 的冲突分支从来不碰 `ignored`，
/// 所以用户忽略过的东西不会因为再扫一次目录就冒出来。
pub fn set_ignored(conn: &Connection, ids: &[String], ignored: bool) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare(
        "UPDATE games \
         SET ignored = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') \
         WHERE id = ?2",
    )?;

    let mut changed = 0usize;
    for id in ids {
        changed += stmt.execute(rusqlite::params![if ignored { 1 } else { 0 }, id])?;
    }
    Ok(changed)
}

/// 从库里彻底删掉若干条，连带清掉它们的 playtime / 会话 / 来源记录。返回删除的行数。
///
/// **只动数据库**：不删硬盘上的游戏目录，也不删封面缓存
/// （封面按 path_hash 寻址、被多个地方共享，删了会影响以后重新扫回来的同一条）。
/// 删完再扫同一个目录，它会作为"新游戏"回来 —— 界面上要如实告诉用户。
pub fn delete_games(conn: &Connection, ids: &[String]) -> rusqlite::Result<usize> {
    let mut deleted = 0usize;
    for id in ids {
        conn.execute("DELETE FROM playtime_sessions WHERE game_hash = ?1", [id])?;
        conn.execute("DELETE FROM playtime WHERE game_hash = ?1", [id])?;
        conn.execute("DELETE FROM game_sources WHERE game_hash = ?1", [id])?;
        deleted += conn.execute("DELETE FROM games WHERE id = ?1", [id])?;
    }
    Ok(deleted)
}

#[cfg(test)]
mod scan_upsert_tests {
    use super::*;
    use crate::db::open_at;
    use crate::db::scanner::{ExeCandidate, ScannedGame};

    fn temp_db(tag: &str) -> Connection {
        let dir = std::env::temp_dir().join("moeshelf-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tag}.db"));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        open_at(&path).expect("建库失败")
    }

    fn sample(hash: &str, engine: &str, exe: &str) -> ScannedGame {
        ScannedGame {
            path_hash: hash.to_string(),
            folder_path: format!("E:\\galgame\\{hash}"),
            folder_name: format!("作品-{hash}"),
            engine: engine.to_string(),
            launch_exe: exe.to_string(),
            launch_candidates: vec![ExeCandidate {
                name: "game.exe".to_string(),
                path: exe.to_string(),
            }],
            file_count: 12,
            dir_size: 4096,
            match_score: 10,
        }
    }

    #[test]
    fn insert_then_update_counted_correctly() {
        let conn = temp_db("scan-upsert");
        let g = sample("aaaa1111", "kirikiri", "E:\\g\\game.exe");

        assert!(upsert_scanned(&conn, &g).unwrap(), "首次应是新增");
        assert!(!upsert_scanned(&conn, &g).unwrap(), "第二次应是更新");

        let row = get_one(&conn, "aaaa1111").unwrap().expect("行应该在");
        assert_eq!(row.title_cn, "作品-aaaa1111", "新行用文件夹名兜底标题");
        assert_eq!(row.engine, "kirikiri");
        assert_eq!(row.dir_size, 4096);
        assert_eq!(row.file_count, 12);
    }

    #[test]
    fn rescan_keeps_scraped_metadata() {
        let conn = temp_db("scan-keep");
        let g = sample("bbbb2222", "kirikiri", "E:\\g\\game.exe");
        upsert_scanned(&conn, &g).unwrap();

        // 模拟刮削/人工编辑填进去的元数据
        let p = GamePatch {
            title_cn: Some("刮削出来的正式名".to_string()),
            developer: Some("Yuzusoft".to_string()),
            tags: Some(vec!["纯爱".to_string()]),
            rating: Some(8.8),
            ..Default::default()
        };
        update_game(&conn, "bbbb2222", &p, true).unwrap();

        // 再扫一次（例如文件数变了）
        let mut g2 = g.clone();
        g2.file_count = 99;
        upsert_scanned(&conn, &g2).unwrap();

        let row = get_one(&conn, "bbbb2222").unwrap().unwrap();
        assert_eq!(row.title_cn, "刮削出来的正式名", "重扫不能覆盖标题");
        assert_eq!(row.developer, "Yuzusoft", "重扫不能覆盖会社");
        assert_eq!(row.tags, vec!["纯爱"]);
        assert_eq!(row.rating, 8.8);
        assert_eq!(row.file_count, 99, "扫描事实应该更新");
    }

    #[test]
    fn locked_launch_exe_is_protected() {
        let conn = temp_db("scan-lock");
        let g = sample("cccc3333", "unknown", "E:\\g\\game.exe");
        upsert_scanned(&conn, &g).unwrap();

        // 用户手动指定了启动项 → 锁定
        let p = GamePatch {
            launch_exe: Some("E:\\g\\汉化版.exe".to_string()),
            ..Default::default()
        };
        update_game(&conn, "cccc3333", &p, true).unwrap();

        // 重扫：扫描器给出的是另一个 exe，引擎也从 unknown 变成了 kirikiri
        let mut g2 = g.clone();
        g2.launch_exe = "E:\\g\\game.exe".to_string();
        g2.engine = "kirikiri".to_string();
        upsert_scanned(&conn, &g2).unwrap();

        let row = get_one(&conn, "cccc3333").unwrap().unwrap();
        assert_eq!(row.launch_exe, "E:\\g\\汉化版.exe", "锁定的启动项不能被扫描覆盖");
        assert_eq!(row.engine, "kirikiri", "引擎是 unknown 时允许被扫描补全");
    }
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;
    use crate::db::open_at;

    /// 拿**真实 dev 库的副本**验证 V1 → V2 升级不会弄坏数据。
    ///
    /// 这一步很重要：`ALTER TABLE ADD COLUMN` 一旦写错，用户下次启动应用就会
    /// 直接打不开库。复制到临时目录做，绝不动原件。
    ///
    /// 手动跑：`cargo test upgrade_real_dev_db_copy -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn upgrade_real_dev_db_copy() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("moeshelf.db");
        if !src.exists() {
            println!("跳过：没有 dev 库");
            return;
        }

        let dir = std::env::temp_dir().join("moeshelf-upgrade-check");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dst = dir.join("moeshelf.db");
        for suffix in ["", "-wal", "-shm"] {
            let from = src.with_file_name(format!("moeshelf.db{suffix}"));
            if from.exists() {
                std::fs::copy(&from, dst.with_file_name(format!("moeshelf.db{suffix}"))).unwrap();
            }
        }

        // 升级前
        let before = {
            let c = Connection::open(&dst).unwrap();
            let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            let n: i64 = c.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).unwrap();
            println!("升级前：schema v{v}，{n} 行");
            (v, n)
        };

        // 走一次真实的 open_at（会跑 V2）
        let conn = open_at(&dst).expect("升级失败");
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).unwrap();
        println!("升级后：schema v{v}，{n} 行");

        assert_eq!(v, crate::db::schema::SCHEMA_VERSION);
        assert_eq!(n, before.1, "升级不能丢行");

        // 新列必须能读，且老数据是 0
        let zeros: i64 = conn
            .query_row("SELECT COUNT(*) FROM games WHERE dir_size = 0", [], |r| r.get(0))
            .unwrap();
        println!("dir_size 仍是 0 的行：{zeros} / {n}");

        // 用真实的 list_all 走一遍，确认结构体映射没崩
        let games = list_all(&conn).expect("list_all 失败");
        assert_eq!(games.len() as i64, n);
        println!("list_all 正常，首条：{} (dirSize={})", games[0].title_cn, games[0].dir_size);
    }
}

#[cfg(test)]
mod maintenance_tests {
    use super::*;
    use crate::db::open_at;
    use crate::db::scanner::{ExeCandidate, ScannedGame};

    fn temp_db(tag: &str) -> Connection {
        let dir = std::env::temp_dir().join("moeshelf-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tag}.db"));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        let conn = open_at(&path).expect("建库失败");
        insert_demo_games(&conn).expect("插演示数据失败");
        conn
    }

    fn scanned(hash: &str) -> ScannedGame {
        ScannedGame {
            path_hash: hash.to_string(),
            folder_path: format!("E:\\galgame\\工具-{hash}"),
            folder_name: format!("GARbro-{hash}"),
            engine: "unknown".to_string(),
            launch_exe: format!("E:\\galgame\\工具-{hash}\\tool.exe"),
            launch_candidates: vec![ExeCandidate {
                name: "tool.exe".to_string(),
                path: format!("E:\\galgame\\工具-{hash}\\tool.exe"),
            }],
            file_count: 3,
            dir_size: 1024,
            match_score: 10,
        }
    }

    #[test]
    fn ignore_hides_from_list_but_keeps_the_row() {
        let conn = temp_db("maint-ignore");
        let ids = vec![DEMO_IDS[0].to_string()];

        assert_eq!(list_all(&conn).unwrap().len(), 4);
        assert_eq!(set_ignored(&conn, &ids, true).unwrap(), 1);

        // 书架上看不到它了，但它还在库里（设置里能恢复）
        assert_eq!(list_all(&conn).unwrap().len(), 3);
        assert_eq!(list_ignored(&conn).unwrap().len(), 1);
        assert_eq!(
            get_one(&conn, DEMO_IDS[0]).unwrap().map(|g| g.title_cn),
            Some("星光记忆".to_string()),
            "忽略不能删行，否则设置里就没法恢复了"
        );

        assert_eq!(set_ignored(&conn, &ids, false).unwrap(), 1);
        assert_eq!(list_all(&conn).unwrap().len(), 4);
        assert!(list_ignored(&conn).unwrap().is_empty());
    }

    #[test]
    fn rescan_does_not_un_ignore() {
        let conn = temp_db("maint-rescan");
        let g = scanned("deadbeef");
        assert!(upsert_scanned(&conn, &g).unwrap(), "第一次应该是新增");

        set_ignored(&conn, &[g.path_hash.clone()], true).unwrap();
        assert_eq!(list_all(&conn).unwrap().len(), 4, "忽略后不该出现在列表里");

        // 再扫一次同一个目录：行还在，只刷新扫描事实，ignored 保持 1
        assert!(!upsert_scanned(&conn, &g).unwrap(), "第二次应该是更新");
        assert_eq!(list_all(&conn).unwrap().len(), 4, "重扫不能把它放出来");
        assert_eq!(list_ignored(&conn).unwrap().len(), 1, "被忽略的那条还在忽略清单里");
    }

    #[test]
    fn delete_removes_row_and_related_records() {
        let conn = temp_db("maint-delete");
        let id = DEMO_IDS[1].to_string();

        conn.execute(
            "INSERT INTO playtime (game_hash, minutes, sessions) VALUES (?1, 120, 3)",
            [&id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playtime_sessions (game_hash, started_at, seconds) VALUES (?1, '2026-01-01T00:00:00Z', 3600)",
            [&id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO game_sources (game_hash, source, fetched_at) VALUES (?1, 'vndb', '2026-01-01T00:00:00Z')",
            [&id],
        )
        .unwrap();

        assert_eq!(delete_games(&conn, &[id.clone()]).unwrap(), 1);

        assert!(get_one(&conn, &id).unwrap().is_none(), "游戏行应该没了");
        let left: i64 = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM playtime WHERE game_hash = ?1) \
                      + (SELECT COUNT(*) FROM playtime_sessions WHERE game_hash = ?1) \
                      + (SELECT COUNT(*) FROM game_sources WHERE game_hash = ?1)",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "关联记录必须一起清掉，不能留孤儿");
        assert_eq!(list_all(&conn).unwrap().len(), 3);
    }

    #[test]
    fn delete_missing_id_counts_zero() {
        let conn = temp_db("maint-delete-missing");
        assert_eq!(delete_games(&conn, &["nope".to_string()]).unwrap(), 0);
    }
}

/* ------------------------------------------------------------ 扫描根目录 */

use crate::models::ScanRoot;

/// 规范化根目录：去空白、去末尾分隔符，但保留盘符根（`E:/`）。
/// 不规范化的话 `E:/galgame` 与 `E:/galgame/` 会被当成两个根目录。
fn normalize_root(p: &str) -> String {
    let t = p.trim();
    let chars: Vec<char> = t.chars().collect();
    let is_drive_root = chars.len() == 3 && chars[1] == ':' && (chars[2] == '\\' || chars[2] == '/');
    if !is_drive_root && (t.ends_with('\\') || t.ends_with('/')) {
        t.trim_end_matches(['\\', '/']).to_string()
    } else {
        t.to_string()
    }
}

pub fn list_scan_roots(conn: &Connection) -> rusqlite::Result<Vec<ScanRoot>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, last_scan_at, added_at FROM scan_roots ORDER BY added_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ScanRoot {
            id: r.get(0)?,
            path: r.get(1)?,
            last_scan_at: r.get(2)?,
            added_at: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// 登记一个根目录。目录必须真实存在（否则用户会盯着一个永远扫不到东西的条目）。
/// 重复登记同一个路径只会返回已有那条，不报错。
pub fn add_scan_root(conn: &Connection, path: &str) -> Result<ScanRoot, String> {
    let normalized = normalize_root(path);
    if normalized.is_empty() {
        return Err("目录路径不能为空".to_string());
    }
    if !std::path::Path::new(&normalized).is_dir() {
        return Err(format!("目录不存在：{normalized}"));
    }

    conn.execute(
        "INSERT INTO scan_roots (path, added_at) VALUES (?1, strftime('%Y-%m-%dT%H:%M:%SZ','now'))
         ON CONFLICT(path) DO NOTHING",
        [&normalized],
    )
    .map_err(|e| format!("登记扫描目录失败：{e}"))?;

    conn.query_row(
        "SELECT id, path, last_scan_at, added_at FROM scan_roots WHERE path = ?1",
        [&normalized],
        |r| {
            Ok(ScanRoot {
                id: r.get(0)?,
                path: r.get(1)?,
                last_scan_at: r.get(2)?,
                added_at: r.get(3)?,
            })
        },
    )
    .map_err(|e| format!("读取刚登记的目录失败：{e}"))
}

pub fn remove_scan_root(conn: &Connection, id: i64) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM scan_roots WHERE id = ?1", [id])
}

/// 扫描成功后打个时间戳。失败不影响扫描结果，所以返回值忽略。
pub fn touch_scan_root(conn: &Connection, path: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE scan_roots SET last_scan_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE path = ?1",
        [&normalize_root(path)],
    )
}

#[cfg(test)]
mod scan_root_tests {
    use super::*;
    use crate::db::open_at;

    fn temp_db(tag: &str) -> Connection {
        let dir = std::env::temp_dir().join("moeshelf-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tag}.db"));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        open_at(&path).expect("建库失败")
    }

    fn real_dir(tag: &str) -> String {
        let d = std::env::temp_dir().join(format!("moeshelf-root-{tag}"));
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().to_string()
    }

    #[test]
    fn add_list_remove_roundtrip() {
        let conn = temp_db("roots");
        let dir = real_dir("a");

        assert_eq!(list_scan_roots(&conn).unwrap().len(), 0);

        let added = add_scan_root(&conn, &dir).expect("登记失败");
        assert_eq!(added.path, dir);
        assert!(added.last_scan_at.is_none(), "刚登记时不该有扫描时间");

        let all = list_scan_roots(&conn).unwrap();
        assert_eq!(all.len(), 1);

        remove_scan_root(&conn, added.id).unwrap();
        assert_eq!(list_scan_roots(&conn).unwrap().len(), 0);
    }

    #[test]
    fn duplicate_path_does_not_create_second_row() {
        let conn = temp_db("roots-dup");
        let dir = real_dir("b");

        let first = add_scan_root(&conn, &dir).unwrap();
        let second = add_scan_root(&conn, &dir).unwrap();

        assert_eq!(first.id, second.id, "同一个路径必须复用同一行");
        assert_eq!(list_scan_roots(&conn).unwrap().len(), 1);
    }

    #[test]
    fn trailing_separator_is_normalized() {
        let conn = temp_db("roots-norm");
        let dir = real_dir("c");

        let a = add_scan_root(&conn, &dir).unwrap();
        let b = add_scan_root(&conn, &format!("{dir}{}", std::path::MAIN_SEPARATOR)).unwrap();

        assert_eq!(a.id, b.id, "多一个结尾分隔符不该算新目录");
        assert_eq!(b.path, dir, "存进去的路径不该带结尾分隔符");
    }

    #[test]
    fn drive_root_keeps_its_separator() {
        // E:/ 去掉分隔符会变成 E:（盘符相对路径），必须保留
        let drive = format!("E:{}", std::path::MAIN_SEPARATOR);
        assert_eq!(normalize_root(&drive), drive);
    }

    #[test]
    fn missing_directory_is_rejected() {
        let conn = temp_db("roots-missing");
        let bad = std::path::Path::new("Z:")
            .join("nope")
            .to_string_lossy()
            .to_string();
        assert!(add_scan_root(&conn, &bad).is_err());
        assert_eq!(list_scan_roots(&conn).unwrap().len(), 0);
    }

    #[test]
    fn touch_updates_last_scan_at() {
        let conn = temp_db("roots-touch");
        let dir = real_dir("d");
        add_scan_root(&conn, &dir).unwrap();

        touch_scan_root(&conn, &dir).unwrap();
        let row = &list_scan_roots(&conn).unwrap()[0];
        assert!(row.last_scan_at.is_some(), "扫描后应写入时间戳");
    }
}

/* ------------------------------------------------------------ 角色与声优 */

/// 读取一部游戏的全部角色，按迁移时的顺序（主角/主要人物在前）。
pub fn list_characters(conn: &Connection, game_id: &str) -> rusqlite::Result<Vec<Character>> {
    let mut stmt = conn.prepare(
        "SELECT name, aliases, role, cv, cv_source, image_url, sort_order
         FROM characters
         WHERE game_hash = ?1
         ORDER BY sort_order ASC, id ASC",
    )?;
    let rows = stmt.query_map([game_id], |r| {
        let aliases_json: String = r.get(1)?;
        Ok(Character {
            name: r.get(0)?,
            aliases: serde_json::from_str(&aliases_json).unwrap_or_default(),
            role: r.get(2)?,
            cv: r.get(3)?,
            cv_source: r.get(4)?,
            image_url: r.get(5)?,
            sort_order: r.get(6)?,
        })
    })?;
    rows.collect()
}

/// 整批替换一部游戏的角色。
///
/// 先删后插而不是逐条 upsert：角色没有稳定主键（VNDB 的 cid 只有 7 条有），
/// 每次刮削拿到的是一份完整名单，整批换掉最省事，也不会留下上一版的多余角色。
/// 调用方负责事务 —— 这里不自己开，避免嵌套。
pub fn replace_characters(
    conn: &Connection,
    game_hash: &str,
    chars: &[Character],
) -> Result<usize, String> {
    conn.execute("DELETE FROM characters WHERE game_hash = ?1", [game_hash])
        .map_err(|e| format!("清理旧角色失败：{e}"))?;

    let mut stmt = conn
        .prepare(
            "INSERT INTO characters
                (game_hash, name, aliases, role, cv, cv_source, image_url, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(|e| format!("准备写入角色失败：{e}"))?;

    for c in chars {
        let aliases = serde_json::to_string(&c.aliases).unwrap_or_else(|_| "[]".into());
        stmt.execute(rusqlite::params![
            game_hash,
            c.name,
            aliases,
            c.role,
            c.cv,
            c.cv_source,
            c.image_url,
            c.sort_order,
        ])
        .map_err(|e| format!("写入角色 {} 失败：{e}", c.name))?;
    }

    Ok(chars.len())
}

#[cfg(test)]
mod character_tests {
    use super::*;
    use crate::db::open_at;

    fn temp_db(tag: &str) -> Connection {
        let dir = std::env::temp_dir().join("moeshelf-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tag}.db"));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        open_at(&path).expect("建库失败")
    }

    fn ch(name: &str, cv: &str, role: &str, order: i64) -> Character {
        Character {
            name: name.to_string(),
            aliases: vec!["Mi-ya".to_string()],
            role: role.to_string(),
            cv: cv.to_string(),
            cv_source: if cv.is_empty() { "" } else { "bangumi" }.to_string(),
            image_url: Some("https://t.vndb.org/ch/87/62187.jpg".to_string()),
            sort_order: order,
        }
    }

    #[test]
    fn replace_then_list_roundtrip() {
        let conn = temp_db("chars-roundtrip");
        assert_eq!(list_characters(&conn, "aaaa").unwrap().len(), 0);

        let n = replace_characters(&conn, "aaaa", &[ch("九條 都", "設楽麻美", "主要人物", 0)]).unwrap();
        assert_eq!(n, 1);

        let list = list_characters(&conn, "aaaa").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "九條 都");
        assert_eq!(list[0].cv, "設楽麻美");
        assert_eq!(list[0].aliases, vec!["Mi-ya"]);
        assert_eq!(list[0].role, "主要人物");
    }

    #[test]
    fn replace_is_idempotent_and_drops_stale_rows() {
        let conn = temp_db("chars-replace");
        replace_characters(&conn, "bbbb", &[ch("A", "", "主角", 0), ch("B", "", "配角", 1)]).unwrap();
        assert_eq!(list_characters(&conn, "bbbb").unwrap().len(), 2);

        // 第二次刮削只拿到一个新名单：旧的两条必须消失，不能累积
        replace_characters(&conn, "bbbb", &[ch("A", "某声优", "主角", 0)]).unwrap();
        let list = list_characters(&conn, "bbbb").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].cv, "某声优");
    }

    #[test]
    fn characters_are_scoped_per_game() {
        let conn = temp_db("chars-scope");
        replace_characters(&conn, "cccc", &[ch("A", "", "", 0)]).unwrap();
        replace_characters(&conn, "dddd", &[ch("B", "", "", 0), ch("C", "", "", 1)]).unwrap();

        assert_eq!(list_characters(&conn, "cccc").unwrap().len(), 1);
        assert_eq!(list_characters(&conn, "dddd").unwrap().len(), 2);
    }

    #[test]
    fn legacy_json_is_tolerated() {
        // 缺 cv / image / aliases 的角色不能导致整批失败
        let raw = serde_json::json!([
            { "name": "九條 都", "role": "主要人物", "image": "https://x/1.jpg" },
            { "name": "", "role": "配角" },
            { "name": "香坂 春風", "cv": "  ", "aliases": ["Empress"] }
        ]);
        let chars = Character::from_legacy_array(raw.as_array().unwrap());
        assert_eq!(chars.len(), 2, "空名字应被跳过");
        assert_eq!(chars[0].cv, "", "缺 cv 应是空串");
        assert_eq!(chars[1].cv, "", "纯空白 cv 应被 trim 成空串");
        assert_eq!(chars[1].aliases, vec!["Empress"]);
        assert_eq!(chars[1].sort_order, 2, "排序保留原始下标");
    }
}

/* ------------------------------------------------------------ 封面替换 */

/// 设置/清空封面。
///
/// 不用 `GamePatch`：补丁语义里 `None` 是"这一列别动"，表达不了"清空"。
/// `lock=true` 时把 `coverUrl` 并入 `locked_fields`（以后刮削不会覆盖），
/// `lock=false`（恢复默认）时把它从锁定列表里摘掉。
pub fn set_cover(
    conn: &Connection,
    id: &str,
    source: Option<&str>,
    lock: bool,
) -> Result<Game, String> {
    let current = get_one(conn, id)
        .map_err(|e| format!("读取游戏失败：{e}"))?
        .ok_or_else(|| format!("找不到该游戏：{id}"))?;

    let mut locked = current.locked_fields.clone();
    if lock {
        if !locked.iter().any(|f| f == "coverUrl") {
            locked.push("coverUrl".to_string());
        }
    } else {
        locked.retain(|f| f != "coverUrl");
    }

    let locked_json = serde_json::to_string(&locked).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE games
            SET cover_url = ?1,
                locked_fields = ?2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
          WHERE id = ?3",
        rusqlite::params![source, locked_json, id],
    )
    .map_err(|e| format!("写入封面失败：{e}"))?;

    get_one(conn, id)
        .map_err(|e| format!("读取游戏失败：{e}"))?
        .ok_or_else(|| format!("更新后读不回来：{id}"))
}

/* ------------------------------------------------------------ 应用设置 */

/// 读一条应用设置；不存在返回 `None`，不是错误。
pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
    let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

/// 写一条应用设置（存在就覆盖）。空值也照写，读的时候按空串处理。
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
