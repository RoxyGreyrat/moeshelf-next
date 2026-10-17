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
///
/// 界面上已经没有入口了（B2 完成后不再需要），命令保留作为运维兜底：
/// 换机器 / 数据损坏时可以临时加个按钮，或直接跑
/// `cargo test migrate_into_dev_db -- --ignored`。
#[tauri::command]
pub fn migrate_legacy(
    db: State<'_, Db>,
    legacy_dir: Option<String>,
) -> Result<MigrateReport, String> {
    let dir = legacy_dir.unwrap_or_else(|| DEFAULT_LEGACY_DIR.to_string());
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    migrate::run(&conn, std::path::Path::new(&dir))
}

/* ------------------------------------------------------------ C 阶段：目录扫描 */

use crate::db::scanner;
use serde::Serialize;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

/// 弹出系统原生的"选择文件夹"对话框，返回用户选中的绝对路径。
///
/// 用户取消返回 `Ok(None)` —— 取消不是错误，前端静默忽略即可。
///
/// 为什么用回调 API + oneshot，而不用 `blocking_pick_folder()`：
/// 阻塞版依赖调用线程所在的消息循环，在 async 命令里调它会把运行时线程占死
/// （Windows 上表现为对话框不出现、窗口白屏）。回调版由主线程自己泵消息，安全。
#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<std::path::PathBuf>>();

    app.dialog()
        .file()
        .set_title(title.unwrap_or_else(|| "选择游戏库根目录".to_string()))
        .pick_folder(move |picked| {
            // 选到的是虚拟位置（拿不到真实路径）时按"取消"处理，避免前端拿到空串去扫描
            let path = picked.and_then(|p| p.into_path().ok());
            let _ = tx.send(path);
        });

    let picked = rx
        .await
        .map_err(|_| "文件夹选择对话框被意外关闭".to_string())?;

    Ok(picked.map(|p| p.to_string_lossy().to_string()))
}

/// 扫描进度事件名。
///
/// 前端用 `listen("scan-progress", ...)` 订阅。节流在 Rust 侧做（约 10 次/秒），
/// 否则一本 200 部的库会产生上万个事件，把 webview 的消息队列打满、进度条反而卡住。
pub const SCAN_PROGRESS_EVENT: &str = "scan-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub root: String,
    pub dirs_scanned: usize,
    pub games_found: usize,
    /// 新增了多少条
    pub inserted: usize,
    /// 更新了多少条（已存在的游戏只刷新扫描事实）
    pub updated: usize,
    pub elapsed_ms: u64,
}

/// 扫描一个根目录，把识别出的游戏写进库。
///
/// 真正的活在 `spawn_blocking` 里跑：扫描是纯 IO，跑在 async 线程上会把整个运行时卡住。
/// `State<Db>` 是在阻塞线程里通过 `app.state()` 现取的 —— 因为 `MutexGuard` 不是 Send，
/// 不能跨 `.await` 持有。
#[tauri::command]
pub async fn scan_directory(app: AppHandle, root: String) -> Result<ScanReport, String> {
    tauri::async_runtime::spawn_blocking(move || scan_and_store(&app, &root))
        .await
        .map_err(|e| format!("扫描线程异常：{e}"))?
}

fn scan_and_store(app: &AppHandle, root: &str) -> Result<ScanReport, String> {
    let started = Instant::now();

    let mut dirs_scanned = 0usize;
    let games = scanner::scan_root(std::path::Path::new(root), |p| {
        dirs_scanned = p.dirs_scanned;
        // 发不出去也不影响扫描（窗口可能已经关了）
        let _ = app.emit(SCAN_PROGRESS_EVENT, p.clone());
    })?;

    let db = app.state::<Db>();
    let mut conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;

    // 整批写在一个事务里：中途报错自动回滚，不会留下"扫了一半"的库
    let tx = conn.transaction().map_err(db_err)?;
    let mut inserted = 0usize;
    let mut updated = 0usize;
    for g in &games {
        if repo::upsert_scanned(&tx, g).map_err(db_err)? {
            inserted += 1;
        } else {
            updated += 1;
        }
    }
    tx.commit().map_err(db_err)?;

    Ok(ScanReport {
        root: root.to_string(),
        dirs_scanned,
        games_found: games.len(),
        inserted,
        updated,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}
