pub mod cover_cache;
pub mod engine_probe;
pub mod migrate;
pub mod repo;
pub mod scanner;
pub mod schema;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// 全局数据库句柄，通过 `app.manage()` 注入，命令里用 `State<Db>` 取。
/// `Connection` 是 Send 但不是 Sync，所以必须裹 Mutex；一条命令一次锁，够用。
pub struct Db(pub Mutex<Connection>);

/// 数据目录：
/// - 开发期固定落在 `src-tauri/data/`（编译期 CARGO_MANIFEST_DIR，方便直接看库文件）
/// - 打包后落在 `%APPDATA%\com.moeshelf.next\data\`（安装目录通常没有写权限）
pub(crate) fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("无法定位应用数据目录：{e}"))?
            .join("data")
    };

    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("无法创建数据目录 {}：{e}", dir.display()))?;

    Ok(dir)
}

/// 打开指定路径的库，并按 `PRAGMA user_version` 升级 schema。
/// 抽成独立函数是为了让单元测试能直接对着临时文件库跑，不用构造 AppHandle。
pub fn open_at(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建目录 {}：{e}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .map_err(|e| format!("打开数据库失败 {}：{e}", path.display()))?;

    // WAL：读写不互相阻塞，异常退出也能恢复
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("初始化 PRAGMA 失败：{e}"))?;

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap_or(0);

    if version < 1 {
        conn.execute_batch(schema::V1_DDL)
            .map_err(|e| format!("建表失败（V1）：{e}"))?;
    }
    if version < 2 {
        // 只对"已经是 v1、没有 dir_size 列"的库执行；全新库会先跑 V1 再跑这里，同样成立
        conn.execute_batch(schema::V2_DDL)
            .map_err(|e| format!("升级失败（V2）：{e}"))?;
    }
    if version < 3 {
        conn.execute_batch(schema::V3_DDL)
            .map_err(|e| format!("升级失败（V3）：{e}"))?;
    }
    if version < 4 {
        conn.execute_batch(schema::V4_DDL)
            .map_err(|e| format!("升级失败（V4）：{e}"))?;
    }
    if version < 5 {
        conn.execute_batch(schema::V5_DDL)
            .map_err(|e| format!("升级失败（V5）：{e}"))?;
    }

    conn.pragma_update(None, "user_version", schema::SCHEMA_VERSION)
        .map_err(|e| format!("写入 user_version 失败：{e}"))?;

    Ok(conn)
}

/// 应用启动时调用：定位数据目录 → 打开库 → 保证 schema 最新。
pub fn init(app: &tauri::AppHandle) -> Result<Db, String> {
    let path = data_dir(app)?.join("moeshelf.db");
    let conn = open_at(&path)?;
    println!("[db] 就绪：{}（schema v{}）", path.display(), schema::SCHEMA_VERSION);
    Ok(Db(Mutex::new(conn)))
}
