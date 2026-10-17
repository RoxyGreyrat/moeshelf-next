use crate::db::{repo, Db};
use crate::models::Game;
use tauri::State;

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
