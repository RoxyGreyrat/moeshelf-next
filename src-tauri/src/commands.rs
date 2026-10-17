use crate::db::migrate::MigrateReport;
use crate::db::{migrate, repo, Db};
use crate::models::Game;
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
