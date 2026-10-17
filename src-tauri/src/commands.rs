use crate::db::migrate::MigrateReport;
use crate::db::{migrate, repo, Db};
use crate::models::{Game, GamePatch};
use tauri::State;

/// 旧库默认位置（产品负责人机器上的 MoeShelf 2.1.0 活库）
pub const DEFAULT_LEGACY_DIR: &str = r"E:\moeshelf\moeshelf-build-2.1.0\data";

fn db_err(e: rusqlite::Error) -> String {
    format!("数据库操作失败：{e}")
}

/// 列出全部游戏
#[tauri::command]
pub fn list_games(db: State<'_, Db>) -> Result<Vec<Game>, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::list_all(&conn).map_err(db_err)
}

/// 插入 4 条演示数据，返回写入后的总行数
#[tauri::command]
pub fn add_dummy_game(db: State<'_, Db>) -> Result<i64, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::insert_demo_games(&conn).map_err(db_err)
}

/// 保存一次编辑：只写补丁里出现的列，并把改动字段并入 `locked_fields`。
///
/// 返回值是**更新后从库里重新 SELECT 出来的整行**，前端拿它直接替换本地状态即可，
/// 不需要再拉一次全表（全表重拉会清空选中，详情面板会被关掉）。
/// 真·持久化验证交给"关掉应用重开"或 F5，那时读到的是同一个库。
#[tauri::command]
pub fn update_game(
    db: State<'_, Db>,
    id: String,
    patch: GamePatch,
    lock: bool,
) -> Result<Game, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::update_game(&conn, &id, &patch, lock)
}

/// 从旧库一次性迁移（旧库只读，全部 INSERT OR REPLACE，可重跑）
#[tauri::command]
pub fn migrate_legacy(
    db: State<'_, Db>,
    legacy_dir: Option<String>,
) -> Result<MigrateReport, String> {
    let dir = legacy_dir.unwrap_or_else(|| DEFAULT_LEGACY_DIR.to_string());
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    migrate::run(&conn, std::path::Path::new(&dir))
}
