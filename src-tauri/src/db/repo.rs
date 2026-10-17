//! 数据访问层。
//!
//! 命令层（`commands.rs`）只负责拿 `State<Db>` 和做错误转换，
//! 真正的 SQL 写在这里 —— 这样可以用 `cargo test` 直接验证，不需要拉起窗口。

use crate::models::{Game, GAME_COLUMNS};
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
}
