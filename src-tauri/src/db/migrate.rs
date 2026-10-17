//! 旧库（MoeShelf 2.1.0）一次性迁移。
//!
//! 设计要点：
//! 1. **旧库只读**：先把 db/wal/shm 三个文件整套复制到临时快照目录，再以 READ_ONLY 打开副本。
//!    正式旧库一个字节都不会被碰（WAL 模式下直接以只读打开原库反而可能失败）。
//! 2. **可重入**：全部用 `INSERT OR REPLACE`，中途报错重跑即可，不会产生重复行。
//!    代价是会覆盖新库里的手工修改 —— 迁移只应该在接真数据那一轮跑一次。
//! 3. **元数据在 JSON 里**：旧库 games 表的 title/developer/cover_url 永远是 NULL，
//!    真数据在 scrape_cache.data_json 里，用 `cache_key 以 ::path_hash 结尾` 关联（实测 211/211 命中）。

use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const FIXTURE_MARK: &str = "moeshelf-scan-fixture";

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateReport {
    pub legacy_dir: String,
    pub snapshot_dir: String,
    pub games_scanned: i64,
    pub skipped_fixture: i64,
    pub imported: i64,
    pub with_metadata: i64,
    pub without_metadata: i64,
    pub playtime_imported: i64,
    pub overrides_applied: i64,
    /// 旧库没标引擎、靠目录指纹补出来的数量
    pub engines_detected: i64,
    pub warnings: Vec<String>,
}

/* ------------------------------------------------------------- 旧库快照 */

fn snapshot(legacy_dir: &Path) -> Result<PathBuf, String> {
    let src = legacy_dir.join("moeshelf.db");
    if !src.exists() {
        return Err(format!("旧库不存在：{}", src.display()));
    }

    let dir = std::env::temp_dir().join("moeshelf-legacy-snapshot");
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建快照目录：{e}"))?;

    for suffix in ["", "-wal", "-shm"] {
        let from = legacy_dir.join(format!("moeshelf.db{suffix}"));
        let to = dir.join(format!("moeshelf.db{suffix}"));
        let _ = std::fs::remove_file(&to);
        if from.exists() {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("复制 {} 失败：{e}", from.display()))?;
        }
    }

    Ok(dir.join("moeshelf.db"))
}

/* ------------------------------------------------------- settings.json 覆盖 */

#[derive(Default)]
struct Overrides {
    title: HashMap<String, String>,
    dev: HashMap<String, String>,
    cover: HashMap<String, String>,
    exe: HashMap<String, String>,
}

fn load_overrides(legacy_dir: &Path) -> Overrides {
    let mut out = Overrides::default();
    let path = legacy_dir.join("settings.json");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return out;
    };
    let Ok(json) = serde_json::from_str::<Value>(&raw) else {
        return out;
    };

    let take = |key: &str, slot: &mut HashMap<String, String>| {
        if let Some(Value::Object(map)) = json.get(key) {
            for (k, v) in map {
                if let Some(s) = v.as_str() {
                    slot.insert(k.clone(), s.to_string());
                }
            }
        }
    };
    take("titleMap", &mut out.title);
    take("devMap", &mut out.dev);
    take("coverMap", &mut out.cover);
    take("exeMap", &mut out.exe);

    out
}

/* ------------------------------------------------------------ 刮削缓存索引 */

/// path_hash -> data_json
fn load_cache(legacy: &Connection) -> HashMap<String, Value> {
    let mut out: HashMap<String, Value> = HashMap::new();
    let Ok(mut stmt) = legacy.prepare(
        "SELECT cache_key, data_json, success FROM scrape_cache \
         WHERE cache_key NOT LIKE 'company::%' AND cache_key LIKE '%::%'",
    ) else {
        return out;
    };

    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
        ))
    }) else {
        return out;
    };

    for (key, data, success) in rows.flatten() {
        let Some(hash) = key.rsplit("::").next() else { continue };
        let exists = out.contains_key(hash);
        // 成功的结果优先；同一 hash 多条时保留第一条成功的
        if exists && success == 0 {
            continue;
        }
        let Some(text) = data else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
        out.insert(hash.to_string(), value);
    }

    out
}

/* --------------------------------------------------------------- 字段解析 */

fn json_vec_strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn str_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// 从 matched_types（JSON 数组）里抽出 `engine:xxx`
fn engine_from(matched_types: &str) -> String {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(matched_types) else {
        return "unknown".into();
    };
    for item in items {
        if let Some(s) = item.as_str() {
            if let Some(engine) = s.strip_prefix("engine:") {
                return engine.trim().to_lowercase();
            }
        }
    }
    "unknown".into()
}

/// exe_candidates 里第一个可执行文件路径。2.1.0 是小写 name/path，旧 C# 库是大写，两种都兼容。
fn first_exe(exe_candidates: &str) -> String {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(exe_candidates) else {
        return String::new();
    };
    for item in items {
        let p = item
            .get("path")
            .or_else(|| item.get("Path"))
            .and_then(|v| v.as_str());
        if let Some(p) = p {
            return p.to_string();
        }
    }
    String::new()
}

fn has_cjk(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(c as u32, 0x4E00..=0x9FFF | 0x3040..=0x30FF | 0xFF00..=0xFFEF))
}

/// 旧 rating 是 0~100，新模型统一 0~10
fn normalize_rating(v: Option<&Value>) -> f64 {
    let raw = match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    };
    if raw <= 0.0 {
        return 0.0;
    }
    let scaled = if raw > 10.0 { raw / 10.0 } else { raw };
    (scaled * 10.0).round() / 10.0
}

fn to_json_text(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::Null) | None => None,
        Some(other) => serde_json::to_string(other).ok(),
    }
}

/* --------------------------------------------------------------- 迁移主体 */

struct LegacyGame {
    path_hash: String,
    folder_name: String,
    folder_path: String,
    file_count: i64,
    exe_candidates: String,
    match_score: f64,
    matched_types: String,
    root_path: String,
    saved_at: String,
    completed: i64,
    completed_at: Option<String>,
    added_at: String,
    updated_at: String,
    first_seen_at: Option<String>,
}

fn read_legacy_games(legacy: &Connection) -> Result<Vec<LegacyGame>, String> {
    let mut stmt = legacy
        .prepare(
            "SELECT path_hash, folder_name, folder_path, file_count, exe_candidates, \
                    match_score, matched_types, root_path, saved_at, completed, completed_at, \
                    added_at, updated_at, first_seen_at \
             FROM games",
        )
        .map_err(|e| format!("读取旧库 games 失败：{e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(LegacyGame {
                path_hash: row.get(0)?,
                folder_name: row.get(1)?,
                folder_path: row.get(2)?,
                file_count: row.get(3)?,
                exe_candidates: row.get(4)?,
                match_score: row.get(5)?,
                matched_types: row.get(6)?,
                root_path: row.get(7)?,
                saved_at: row.get(8)?,
                completed: row.get(9)?,
                completed_at: row.get(10)?,
                added_at: row.get(11)?,
                updated_at: row.get(12)?,
                first_seen_at: row.get(13)?,
            })
        })
        .map_err(|e| format!("查询旧库 games 失败：{e}"))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("解析旧库行失败：{e}"))?);
    }
    Ok(out)
}

pub fn run(target: &Connection, legacy_dir: &Path) -> Result<MigrateReport, String> {
    let snapshot_path = snapshot(legacy_dir)?;
    let legacy = Connection::open_with_flags(&snapshot_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("以只读方式打开旧库快照失败：{e}"))?;

    let overrides = load_overrides(legacy_dir);
    let cache = load_cache(&legacy);
    let legacy_games = read_legacy_games(&legacy)?;

    let mut report = MigrateReport {
        legacy_dir: legacy_dir.display().to_string(),
        snapshot_dir: snapshot_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        games_scanned: legacy_games.len() as i64,
        ..Default::default()
    };

    let tx = target
        .unchecked_transaction()
        .map_err(|e| format!("开启事务失败：{e}"))?;

    // 先清掉开发期的演示数据，否则真实库里会混进 4 条假游戏
    let purged = crate::db::repo::purge_demo_games(&tx)
        .map_err(|e| format!("清理演示数据失败：{e}"))?;
    if purged > 0 {
        report
            .warnings
            .push(format!("已清理 {purged} 条开发期演示数据"));
    }

    {
        let mut stmt = tx
            .prepare(
                "INSERT OR REPLACE INTO games (
                    id, path_hash, title, title_cn, original_title, developer, released,
                    engine, platform, tags, rating, rating_source, description, cn_description,
                    cover_url, launch_exe, folder_path, folder_name, file_count, match_score,
                    root_path, completed, completed_at, saved_at, added_at, updated_at,
                    first_seen_at, locked_fields, field_sources, manual_match, ignored,
                    vndb_id, bangumi_id
                ) VALUES (
                    ?1, ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, 'windows', ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18,
                    ?19, ?20, ?21, ?22, ?23, ?24,
                    ?25, ?26, ?27, ?28, 0,
                    ?29, ?30
                )",
            )
            .map_err(|e| format!("准备写入语句失败：{e}"))?;

        for g in &legacy_games {
            if g.folder_path.contains(FIXTURE_MARK) {
                report.skipped_fixture += 1;
                continue;
            }

            let meta = cache.get(&g.path_hash);
            let mut locked: Vec<String> = Vec::new();
            let mut overrides_hit = 0;

            // 元数据基线（来自刮削缓存）
            let mut title = str_of(meta.and_then(|m| m.get("title")));
            let mut title_cn = {
                let official = str_of(meta.and_then(|m| m.get("officialCnTitle")));
                if official.is_empty() {
                    str_of(meta.and_then(|m| m.get("cnTitle")))
                } else {
                    official
                }
            };
            let mut developer = {
                let devs = json_vec_strings(meta.and_then(|m| m.get("developers")));
                devs.first().cloned().unwrap_or_default()
            };
            let mut cover_url = str_of(meta.and_then(|m| m.get("coverUrl")));
            let mut launch_exe = first_exe(&g.exe_candidates);

            // 人工覆盖：优先级最高，并记进 locked_fields
            if let Some(v) = overrides.title.get(&g.path_hash) {
                if has_cjk(v) {
                    title_cn = v.clone();
                    locked.push("titleCn".into());
                } else {
                    title = v.clone();
                    locked.push("title".into());
                }
                overrides_hit += 1;
            }
            if let Some(v) = overrides.dev.get(&g.path_hash) {
                developer = v.clone();
                locked.push("developer".into());
                overrides_hit += 1;
            }
            if let Some(v) = overrides.cover.get(&g.path_hash) {
                cover_url = v.clone();
                locked.push("coverUrl".into());
                overrides_hit += 1;
            }
            if let Some(v) = overrides.exe.get(&g.path_hash) {
                launch_exe = v.clone();
                locked.push("launchExe".into());
                overrides_hit += 1;
            }
            report.overrides_applied += overrides_hit;

            // 显示标题三级兜底：中文名 → 外文名 → 文件夹名。
            // 卡片主标题固定渲染 title_cn，缺了就会显示空白，所以这里必须填满。
            if title_cn.is_empty() {
                title_cn = if title.is_empty() {
                    g.folder_name.clone()
                } else {
                    title.clone()
                };
            }
            if meta.is_some() {
                report.with_metadata += 1;
            } else {
                report.without_metadata += 1;
            }

            // 引擎：旧库标记 → 目录指纹 → 主程序内容特征串
            let mut engine = engine_from(&g.matched_types);
            if engine == "unknown" {
                let dir = std::path::Path::new(&g.folder_path);
                let found = super::engine_probe::detect(dir).or_else(|| {
                    if launch_exe.is_empty() {
                        None
                    } else {
                        super::engine_probe::detect_from_exe(std::path::Path::new(&launch_exe))
                    }
                });
                if let Some(found) = found {
                    engine = found.to_string();
                    report.engines_detected += 1;
                }
            }

            let tags = serde_json::to_string(&json_vec_strings(
                meta.and_then(|m| m.get("genres")),
            ))
            .unwrap_or_else(|_| "[]".into());

            let first_seen = g
                .first_seen_at
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| g.added_at.clone());

            stmt.execute(params![
                g.path_hash,                                   // ?1 id / path_hash
                title,                                         // ?2
                title_cn,                                      // ?3
                str_of(meta.and_then(|m| m.get("originalTitle"))), // ?4
                developer,                                     // ?5
                str_of(meta.and_then(|m| m.get("released"))),  // ?6
                engine,                                        // ?7
                tags,                                          // ?8
                normalize_rating(meta.and_then(|m| m.get("rating"))), // ?9
                str_of(meta.and_then(|m| m.get("ratingSource"))), // ?10
                str_of(meta.and_then(|m| m.get("description"))),  // ?11
                str_of(meta.and_then(|m| m.get("cnDescription"))),// ?12
                if cover_url.is_empty() { None } else { Some(cover_url) }, // ?13
                launch_exe,                                    // ?14
                g.folder_path,                                 // ?15
                g.folder_name,                                 // ?16
                g.file_count,                                  // ?17
                g.match_score,                                 // ?18
                g.root_path,                                   // ?19
                g.completed,                                   // ?20
                g.completed_at,                                // ?21
                g.saved_at,                                    // ?22
                g.added_at,                                    // ?23
                g.updated_at,                                  // ?24
                first_seen,                                    // ?25
                serde_json::to_string(&locked).unwrap_or_else(|_| "[]".into()), // ?26
                to_json_text(meta.and_then(|m| m.get("fieldSources")))
                    .unwrap_or_else(|| "{}".into()),           // ?27
                to_json_text(meta.and_then(|m| m.get("manualMatch"))), // ?28
                str_of(meta.and_then(|m| m.get("vndbId"))),    // ?29
                meta.and_then(|m| m.get("bgmSubjectId"))
                    .and_then(|v| v.as_i64()),                 // ?30
            ])
            .map_err(|e| format!("写入 {} 失败：{e}", g.folder_name))?;

            report.imported += 1;
        }
    }

    // 游玩时长：只搬新库里存在对应游戏的记录
    if let Ok(mut stmt) = legacy.prepare(
        "SELECT game_hash, minutes, sessions, last_played FROM playtime",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        }) {
            for (hash, minutes, sessions, last_played) in rows.flatten() {
                let exists: i64 = tx
                    .query_row("SELECT COUNT(*) FROM games WHERE path_hash = ?1", [&hash], |r| {
                        r.get(0)
                    })
                    .unwrap_or(0);
                if exists == 0 {
                    continue;
                }
                let ok = tx.execute(
                    "INSERT OR REPLACE INTO playtime (game_hash, minutes, sessions, last_played) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![hash, minutes, sessions, last_played],
                );
                if ok.is_ok() {
                    report.playtime_imported += 1;
                }
            }
        }
    }

    tx.commit().map_err(|e| format!("提交事务失败：{e}"))?;

    if report.without_metadata > 0 {
        report.warnings.push(format!(
            "{} 部没有刮削元数据，已用文件夹名兜底",
            report.without_metadata
        ));
    }

    Ok(report)
}

/* ------------------------------------------------------------------ 测试 */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_parsing() {
        assert_eq!(
            engine_from(r#"[".exe",".xp3","engine:kirikiri"]"#),
            "kirikiri"
        );
        assert_eq!(engine_from(r#"[".exe",".txt"]"#), "unknown");
        assert_eq!(engine_from("not json"), "unknown");
    }

    #[test]
    fn exe_candidates_case_insensitive() {
        let lower = r#"[{"name":"a.exe","path":"C:\\a.exe"}]"#;
        let upper = r#"[{"Name":"b.exe","Path":"C:\\b.exe"}]"#;
        assert_eq!(first_exe(lower), r"C:\a.exe");
        assert_eq!(first_exe(upper), r"C:\b.exe");
    }

    #[test]
    fn rating_is_scaled_to_ten() {
        assert_eq!(normalize_rating(Some(&Value::from(92))), 9.2);
        assert_eq!(normalize_rating(Some(&Value::from(68))), 6.8);
        assert_eq!(normalize_rating(Some(&Value::from(9.3))), 9.3);
        assert_eq!(normalize_rating(None), 0.0);
    }

    #[test]
    fn cjk_detection() {
        assert!(has_cjk("白色相簿2"));
        assert!(has_cjk("サクラノ詩"));
        assert!(!has_cjk("KANADE"));
    }

    /// 开发期专用：把旧库迁移进应用真正使用的 dev 数据库（`src-tauri/data/moeshelf.db`）。
    /// 等价于点界面上的「迁移旧库」按钮，只是省得手动点。
    /// 跑法：cargo test migrate_into_dev_db -- --ignored --nocapture
    /// ⚠️ 要先把应用关掉，否则连接会争锁。
    #[test]
    #[ignore = "开发期工具，会写入 src-tauri/data/moeshelf.db"]
    fn migrate_into_dev_db() {
        let legacy = Path::new(r"E:\moeshelf\moeshelf-build-2.1.0\data");
        if !legacy.join("moeshelf.db").exists() {
            println!("跳过：本机没有旧库");
            return;
        }
        let db_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("moeshelf.db");
        let conn = crate::db::open_at(&db_path).expect("打开 dev 库失败");
        let report = run(&conn, legacy).expect("迁移失败");
        println!("迁移报告：{report:#?}");
        println!("dev 库位置：{}", db_path.display());
    }

    /// 需要本机存在旧库，默认跳过。手动跑：
    /// cd src-tauri && cargo test migrate_real_legacy -- --ignored --nocapture
    #[test]
    #[ignore = "依赖本机旧库"]
    fn migrate_real_legacy() {
        let legacy = Path::new(r"E:\moeshelf\moeshelf-build-2.1.0\data");
        if !legacy.join("moeshelf.db").exists() {
            println!("跳过：本机没有旧库");
            return;
        }

        let dir = std::env::temp_dir().join("moeshelf-migrate-test");
        std::fs::create_dir_all(&dir).unwrap();
        let target_path = dir.join("target.db");
        for s in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{s}", target_path.display()));
        }

        let conn = crate::db::open_at(&target_path).unwrap();
        let report = run(&conn, legacy).unwrap();
        println!("迁移报告：{report:#?}");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
            .unwrap();
        assert!(count > 200, "应导入 200+ 条，实际 {count}");
        assert_eq!(report.imported, count);

        let with_title: i64 = conn
            .query_row("SELECT COUNT(*) FROM games WHERE title_cn != ''", [], |r| r.get(0))
            .unwrap();
        assert_eq!(with_title, count, "每部游戏都必须有可显示的标题");

        let engines: i64 = conn
            .query_row("SELECT COUNT(DISTINCT engine) FROM games", [], |r| r.get(0))
            .unwrap();
        assert!(engines >= 4, "引擎种类应 >= 4，实际 {engines}");

        let locked: i64 = conn
            .query_row("SELECT COUNT(*) FROM games WHERE locked_fields != '[]'", [], |r| r.get(0))
            .unwrap();
        assert!(locked > 0, "应有人工覆盖被写入 locked_fields");

        let play: i64 = conn
            .query_row("SELECT COUNT(*) FROM playtime", [], |r| r.get(0))
            .unwrap();
        println!("playtime 导入 {play} 条");

        let unknown: i64 = conn
            .query_row("SELECT COUNT(*) FROM games WHERE engine = 'unknown'", [], |r| r.get(0))
            .unwrap();
        println!(
            "引擎：目录指纹补出 {} 部，仍未知 {} 部",
            report.engines_detected, unknown
        );
        assert!(
            report.engines_detected > 0,
            "应该靠目录指纹补出一批引擎，实际 {}",
            report.engines_detected
        );
        assert!(unknown < 111, "未知引擎应少于旧库的 111 部，实际 {unknown}");
    }
}
