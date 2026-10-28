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

/// 列出被忽略的条目（「设置 → 已忽略」里显示，可以恢复）。
#[tauri::command]
pub fn list_ignored_games(db: State<'_, Db>) -> Result<Vec<Game>, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::list_ignored(&conn).map_err(db_err)
}

/// 忽略 / 取消忽略若干条。返回真正改动的行数。
///
/// 整批包在一个事务里：要么全忽略，要么一条都不动，不会出现"忽略了一半"。
#[tauri::command]
pub fn set_games_ignored(
    db: State<'_, Db>,
    ids: Vec<String>,
    ignored: bool,
) -> Result<usize, String> {
    let mut conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let tx = conn.transaction().map_err(db_err)?;
    let changed = repo::set_ignored(&tx, &ids, ignored).map_err(db_err)?;
    tx.commit().map_err(db_err)?;
    Ok(changed)
}

/// 从库里彻底删除若干条（连带 playtime / 会话 / 来源记录）。
///
/// ⚠️ 只删数据库：不删硬盘上的游戏目录，也不删封面缓存。
/// 删完再扫同一个目录，它会作为"新游戏"回来 —— 界面上必须如实告诉用户。
#[tauri::command]
pub fn delete_games(db: State<'_, Db>, ids: Vec<String>) -> Result<usize, String> {
    let mut conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let tx = conn.transaction().map_err(db_err)?;
    let deleted = repo::delete_games(&tx, &ids).map_err(db_err)?;
    tx.commit().map_err(db_err)?;
    Ok(deleted)
}

/// 从旧库一次性迁移（旧库只读，全部 INSERT OR REPLACE，可重跑）///
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
    /// 新增行的 id（= path_hash）。前端拿它去跑「自动匹配」，不用再全表比对。
    pub inserted_ids: Vec<String>,
    /// 是不是被用户中途取消的（已扫到的部分照常入库）
    pub cancelled: bool,
}

/// 扫描取消开关。
///
/// 放在 app 状态里而不是命令参数里：扫描是 `spawn_blocking` 里跑的循环，
/// 前端只能从另一个命令改这个原子布尔，扫描线程每进一个目录读一次。
pub struct ScanCancel(pub std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Default for ScanCancel {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)))
    }
}

/// 取消正在进行的扫描。没有扫描在跑时调用也无害。
#[tauri::command]
pub fn cancel_scan(state: State<'_, ScanCancel>) -> bool {
    state
        .0
        .store(true, std::sync::atomic::Ordering::SeqCst);
    true
}

/// 扫描一个根目录，把识别出的游戏写进库。
///
/// 真正的活在 `spawn_blocking` 里跑：扫描是纯 IO，跑在 async 线程上会把整个运行时卡住。
/// `State<Db>` 是在阻塞线程里通过 `app.state()` 现取的 —— 因为 `MutexGuard` 不是 Send，
/// 不能跨 `.await` 持有。
#[tauri::command]
pub async fn scan_directory(app: AppHandle, root: String) -> Result<ScanReport, String> {
    // Arc 先克隆出来：State<'_> 不能跨线程，原子布尔可以
    let cancel = app.state::<ScanCancel>().0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        cancel.store(false, std::sync::atomic::Ordering::SeqCst);
        scan_and_store(&app, &root, &cancel)
    })
    .await
    .map_err(|e| format!("扫描线程异常：{e}"))?
}

fn scan_and_store(
    app: &AppHandle,
    root: &str,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<ScanReport, String> {
    let started = Instant::now();

    let mut dirs_scanned = 0usize;
    let cancelled = std::sync::Arc::clone(cancel);
    let (games, was_cancelled) = scanner::scan_root_cancellable(
        std::path::Path::new(root),
        |p| {
            dirs_scanned = p.dirs_scanned;
            // 发不出去也不影响扫描（窗口可能已经关了）
            let _ = app.emit(SCAN_PROGRESS_EVENT, p.clone());
        },
        &|| cancelled.load(std::sync::atomic::Ordering::SeqCst),
    )?;

    let db = app.state::<Db>();
    let mut conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;

    // 整批写在一个事务里：中途报错自动回滚，不会留下"扫了一半"的库
    let tx = conn.transaction().map_err(db_err)?;
    let mut inserted = 0usize;
    let mut updated = 0usize;
    let mut inserted_ids: Vec<String> = Vec::new();
    for g in &games {
        if repo::upsert_scanned(&tx, g).map_err(db_err)? {
            inserted += 1;
            inserted_ids.push(g.path_hash.clone());
        } else {
            updated += 1;
        }
    }
    tx.commit().map_err(db_err)?;

    // 扫过就记住这个目录：下次从顶栏直接选，不用再翻磁盘。
    // 两步都忽略错误 —— 记不住不影响这次扫描的结果。
    let _ = repo::add_scan_root(&conn, root);
    let _ = repo::touch_scan_root(&conn, root);

    Ok(ScanReport {
        root: root.to_string(),
        dirs_scanned,
        games_found: games.len(),
        inserted,
        updated,
        elapsed_ms: started.elapsed().as_millis() as u64,
        inserted_ids,
        cancelled: was_cancelled,
    })
}

/* ---------------------------------------------------- 设置页要用的命令 */

use crate::models::ScanRoot;

/// 已登记的扫描根目录
#[tauri::command]
pub fn list_scan_roots(db: State<'_, Db>) -> Result<Vec<ScanRoot>, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::list_scan_roots(&conn).map_err(db_err)
}

/// 登记一个扫描根目录（目录必须存在）
#[tauri::command]
pub fn add_scan_root(db: State<'_, Db>, path: String) -> Result<ScanRoot, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::add_scan_root(&conn, &path)
}

#[tauri::command]
pub fn remove_scan_root(db: State<'_, Db>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::remove_scan_root(&conn, id).map_err(db_err)?;
    Ok(())
}

/// 用系统默认程序打开一个目录（资源管理器）。
///
/// 走 opener 插件的 **Rust API**，不经过前端 ACL，所以 capabilities 里不需要额外权限。
#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("路径不存在：{path}"));
    }
    tauri_plugin_opener::open_path(&path, None::<&str>)
        .map_err(|e| format!("打开目录失败：{e}"))
}

/// 在资源管理器里**定位**到某一项（打开父目录并选中它）
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("路径不存在：{path}"));
    }
    tauri_plugin_opener::reveal_item_in_dir(&path)
        .map_err(|e| format!("定位失败：{e}"))
}

/// 设置页「数据与关于」要展示的事实。全部现算，不做缓存 —— 打开一次设置才读一次。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// 应用版本（来自 Cargo.toml）
    pub version: String,
    /// 数据库 schema 版本
    pub schema_version: i64,
    pub data_dir: String,
    pub db_path: String,
    pub db_size: u64,
    /// 封面缓存目录（**旧项目的目录**，方案 A 直读）
    pub cover_dir: String,
    pub cover_dir_exists: bool,
    /// 缓存里有多少张封面
    pub cover_count: usize,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> Result<AppInfo, String> {
    let dir = crate::db::data_dir(&app)?;
    let db_path = dir.join("moeshelf.db");
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    let cover_dir = crate::db::cover_cache::dir();
    let cover_dir_exists = cover_dir.is_dir();
    let cover_count = if cover_dir_exists {
        std::fs::read_dir(&cover_dir)
            .map(|it| {
                it.flatten()
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .to_ascii_lowercase()
                            .ends_with(".bin")
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: crate::db::schema::SCHEMA_VERSION,
        data_dir: dir.to_string_lossy().to_string(),
        db_path: db_path.to_string_lossy().to_string(),
        db_size,
        cover_dir: cover_dir.to_string_lossy().to_string(),
        cover_dir_exists,
        cover_count,
    })
}

/* ------------------------------------------------------------ 数据库备份 */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupReport {
    /// 备份文件完整路径
    pub path: String,
    pub bytes: u64,
}

/// 把当前库备份到指定目录，文件名带时间戳。
///
/// 用 SQLite 自己的 `VACUUM INTO`：它会在一个只读快照上写出一份**完整且一致**的库，
/// 不需要停机，也不怕 WAL 里还有没落盘的事务（直接复制 .db 文件会丢掉这部分）。
#[tauri::command]
pub fn backup_database(db: State<'_, Db>, dest_dir: String) -> Result<BackupReport, String> {
    let dir = std::path::PathBuf::from(&dest_dir);
    if !dir.is_dir() {
        return Err(format!("目标目录不存在：{dest_dir}"));
    }

    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    write_backup(&conn, &dir)
}

/// 备份的实际动作。抽出来是为了能对着临时库写单元测试（不需要构造 AppHandle）。
fn write_backup(
    conn: &rusqlite::Connection,
    dir: &std::path::Path,
) -> Result<BackupReport, String> {
    let stamp: String = conn
        .query_row("SELECT strftime('%Y%m%d-%H%M%S','now','localtime')", [], |r| {
            r.get(0)
        })
        .map_err(db_err)?;

    let file = dir.join(format!("moeshelf-{stamp}.db"));
    if file.exists() {
        return Err("同一秒内已经备份过一次了，请稍后再试".to_string());
    }

    conn.execute("VACUUM INTO ?1", [file.to_string_lossy().to_string()])
        .map_err(|e| format!("备份失败：{e}"))?;

    let bytes = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);

    Ok(BackupReport {
        path: file.to_string_lossy().to_string(),
        bytes,
    })
}

#[cfg(test)]
mod backup_tests {
    use super::*;

    /// 备份出来的文件必须是一份能打开、行数一致的库
    /// （VACUUM INTO 的意义就在这里：不是"复制了个文件"，而是一致快照）
    #[test]
    fn backup_produces_a_valid_copy() {
        let dir = std::env::temp_dir().join("moeshelf-backup-test");
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("source.db");
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", src_path.display()));
        }
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("moeshelf-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }

        let conn = crate::db::open_at(&src_path).expect("建库失败");
        crate::db::repo::insert_demo_games(&conn).expect("插演示数据失败");

        let report = write_backup(&conn, &dir).expect("备份失败");
        assert!(std::path::Path::new(&report.path).is_file(), "备份文件应存在");
        assert!(report.bytes > 0, "备份文件不应为空");

        // 打开备份，行数必须和源库一致
        let copy = rusqlite::Connection::open(&report.path).expect("备份文件打不开");
        let count: i64 = copy
            .query_row("SELECT count(*) FROM games", [], |r| r.get(0))
            .expect("统计失败");
        assert_eq!(
            count as usize,
            crate::db::repo::DEMO_IDS.len(),
            "备份里的游戏条数应与源库一致"
        );
    }

    #[test]
    fn backup_rejects_missing_dir() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let missing = std::env::temp_dir().join("moeshelf-backup-not-here");
        let _ = std::fs::remove_dir_all(&missing);
        // 命令层会先校验目录；这里确认 write_backup 本身对不存在的目录也不会 panic
        let err = write_backup(&conn, &missing);
        assert!(err.is_err(), "目录不存在时应当报错");
    }
}

/* ------------------------------------------------------------ 启动游戏 */

/// 游戏退出事件：前端收到后刷新"正在运行"状态与时长
pub const GAME_EXITED_EVENT: &str = "game-exited";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeInfo {
    /// 总时长（分钟）
    pub minutes: f64,
    /// 启动次数
    pub sessions: i64,
    pub last_played: Option<String>,
    /// 本次应用会话内是否在运行
    pub running: bool,
    /// 已运行秒数（running 时有效，前端据此跑计时器）
    pub running_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExeCandidateInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchInfo {
    pub game_id: String,
    pub exe: String,
    pub pid: u32,
}

struct RunningGame {
    pid: u32,
    started: Instant,
}

fn running_map(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, RunningGame>> {
    static RUNNING: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, RunningGame>>,
    > = std::sync::OnceLock::new();
    RUNNING.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn playtime_info(db: &Db, game_id: &str) -> Result<PlaytimeInfo, String> {
    let (minutes, sessions, last_played) = {
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        repo::get_playtime(&conn, game_id)
            .map_err(db_err)?
            .unwrap_or((0.0, 0, None))
    };

    let (running, running_seconds) = match running_map().lock() {
        Ok(map) => match map.get(game_id) {
            Some(r) => (true, r.started.elapsed().as_secs()),
            None => (false, 0),
        },
        Err(_) => (false, 0),
    };

    Ok(PlaytimeInfo {
        minutes,
        sessions,
        last_played,
        running,
        running_seconds,
    })
}

#[tauri::command]
pub fn get_playtime(db: State<'_, Db>, id: String) -> Result<PlaytimeInfo, String> {
    playtime_info(db.inner(), &id)
}

/// 这个游戏目录里有哪些可启动的程序（首次启动让用户挑）。
///
/// 扫描器猜的主程序可能不准，但"目录里有哪些 exe"是事实。
/// 当前记录的主程序即使不在候选里（比如在更深的子目录），也会排到第一位。
#[tauri::command]
pub fn list_exe_candidates(
    db: State<'_, Db>,
    id: String,
) -> Result<Vec<ExeCandidateInfo>, String> {
    let game = {
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        repo::get_one(&conn, &id)
            .map_err(db_err)?
            .ok_or_else(|| format!("找不到该游戏：{id}"))?
    };

    let dir = std::path::PathBuf::from(&game.folder_path);
    if !dir.is_dir() {
        return Err("游戏目录不存在，可能被移动或改名了".to_string());
    }

    let mut out: Vec<ExeCandidateInfo> = Vec::new();

    let current = game.launch_exe.trim();
    if !current.is_empty() {
        let path = std::path::PathBuf::from(current);
        if path.is_file() {
            out.push(ExeCandidateInfo {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| current.to_string()),
                path: current.to_string(),
                size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            });
        }
    }

    for (name, path) in scanner::exe_candidates(&dir) {
        let text = path.to_string_lossy().to_string();
        if out.iter().any(|e| e.path.eq_ignore_ascii_case(&text)) {
            continue;
        }
        out.push(ExeCandidateInfo {
            name,
            size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            path: text,
        });
    }

    Ok(out)
}

/// 启动游戏。
///
/// - 已经在运行 → 直接返回当前 pid，不重复启动；
/// - 传了 `exe` → 认为用户确认了启动项，写回 `launch_exe` **并锁定**，
///   以后重扫不会把它改掉（也正是"首次启动需要确认、之后可以直接启动"的实现）；
/// - 真正跑起来之后才记一次启动，并开一条会话；进程退出时回填时长。
#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    id: String,
    exe: Option<String>,
) -> Result<LaunchInfo, String> {
    tauri::async_runtime::spawn_blocking(move || run_launch(&app, &id, exe.as_deref()))
        .await
        .map_err(|e| format!("启动线程异常：{e}"))?
}

fn run_launch(app: &AppHandle, id: &str, exe_override: Option<&str>) -> Result<LaunchInfo, String> {
    // 已经在运行：不重复启动（连点也不会开出第二个进程）
    if let Ok(map) = running_map().lock() {
        if let Some(r) = map.get(id) {
            return Ok(LaunchInfo {
                game_id: id.to_string(),
                exe: String::new(),
                pid: r.pid,
            });
        }
    }

    let game = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        repo::get_one(&conn, id)
            .map_err(db_err)?
            .ok_or_else(|| format!("找不到该游戏：{id}"))?
    };

    let chosen = exe_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let current = game.launch_exe.trim();
            if current.is_empty() {
                None
            } else {
                Some(current.to_string())
            }
        })
        .ok_or_else(|| "还没有选择启动程序".to_string())?;

    let exe_path = std::path::PathBuf::from(&chosen);
    if !exe_path.is_file() {
        return Err(format!("找不到启动程序：{chosen}"));
    }

    let dir = std::path::PathBuf::from(&game.folder_path);
    if !dir.is_dir() {
        return Err("游戏目录不存在，可能被移动或改名了".to_string());
    }

    // 用户确认过启动项：写回并锁定（锁了以后扫描/刮削都不会改它）
    if exe_override.is_some() && chosen != game.launch_exe {
        let db = app.state::<Db>();
        if let Ok(conn) = db.0.lock() {
            let mut patch = GamePatch::default();
            patch.launch_exe = Some(chosen.clone());
            let _ = repo::update_game(&conn, id, &patch, true);
        }
    }

    let mut child = std::process::Command::new(&exe_path)
        .current_dir(&dir)
        .spawn()
        .map_err(|e| format!("启动失败：{e}"))?;
    let pid = child.id();

    let session_id = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        let sid = repo::start_session(&conn, id).map_err(db_err)?;
        repo::mark_launched(&conn, id).map_err(db_err)?;
        sid
    };

    if let Ok(mut map) = running_map().lock() {
        map.insert(
            id.to_string(),
            RunningGame {
                pid,
                started: Instant::now(),
            },
        );
    }

    // 等进程退出（阻塞线程，不占 async 运行时）
    let app2 = app.clone();
    let game_id = id.to_string();
    std::thread::spawn(move || {
        let _ = child.wait();

        // 有些 galgame 是"启动器 → 正片"两层进程：父进程退出后，子进程可能还在。
        // 这里不追进程树（v1 的取舍），至少把父进程的时长记准确。
        let seconds = running_map()
            .lock()
            .ok()
            .and_then(|mut map| map.remove(&game_id))
            .map(|r| r.started.elapsed().as_secs())
            .unwrap_or(0);

        {
            let db = app2.state::<Db>();
            if let Ok(conn) = db.0.lock() {
                let _ = repo::finish_session(&conn, session_id, seconds as i64);
            }
        }

        let _ = app2.emit(
            GAME_EXITED_EVENT,
            serde_json::json!({ "gameId": game_id, "seconds": seconds }),
        );
    });

    Ok(LaunchInfo {
        game_id: id.to_string(),
        exe: chosen,
        pid,
    })
}

/// 结束游戏（连同子进程）。
///
/// `/T` 是必要的：不少 galgame 由一个启动器再拉起正片，只杀父进程会留下黑屏的游戏进程。
#[tauri::command]
pub fn stop_game(id: String) -> Result<(), String> {
    let pid = {
        let map = running_map().lock().map_err(|_| "运行状态被占用".to_string())?;
        map.get(&id).map(|r| r.pid)
    }
    .ok_or_else(|| "这个游戏没有在运行".to_string())?;

    let status = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
        .map_err(|e| format!("结束进程失败：{e}"))?;

    if !status.success() {
        return Err("进程可能已经退出".to_string());
    }
    Ok(())
}

/* ------------------------------------------------------------ 全屏详情页 */

/// 全屏详情页一次要用的全部数据。
///
/// 合成一个命令而不是让前端发三次 invoke：
/// 打开详情页时游戏、角色、时长必须同时到位，否则会出现"标题先出来、角色区晚 300ms"的跳变。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDetail {
    pub game: Game,
    pub characters: Vec<crate::models::Character>,
    /// 游玩记录：总时长 / 启动次数 / 是否正在运行。
    /// 和游戏、角色一次返回，详情页不会出现“时长晚半秒才到”的跳变。
    pub playtime: PlaytimeInfo,
}

#[tauri::command]
pub fn get_game_detail(db: State<'_, Db>, id: String) -> Result<GameDetail, String> {
    let (game, characters) = {
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        let game = repo::get_one(&conn, &id)
            .map_err(db_err)?
            .ok_or_else(|| format!("找不到该游戏：{id}"))?;
        let characters = repo::list_characters(&conn, &id).map_err(db_err)?;
        (game, characters)
    };

    // 注意锁要在这里之前释放：playtime_info 内部还要再取一次锁
    let playtime = playtime_info(db.inner(), &id)?;

    Ok(GameDetail {
        game,
        characters,
        playtime,
    })
}

/* ------------------------------------------------------------ 自制路径选择器 */

/// 路径选择器里的一行（文件夹或文件）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathListing {
    /// 当前目录；`null` 表示"还没进任何目录"，此时只列盘符
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<PathEntry>,
}

/// 列出所有盘符。Windows 没有便宜的 API，直接试 A:~Z: 是否可进入
fn list_drives() -> Vec<PathEntry> {
    ('A'..='Z')
        .filter_map(|c| {
            let path = format!("{c}:\\");
            if std::path::Path::new(&path).is_dir() {
                Some(PathEntry {
                    name: format!("{c}:"),
                    path,
                    is_dir: true,
                })
            } else {
                None
            }
        })
        .collect()
}

/// 列目录。`path` 为 None 时返回盘符列表。
///
/// 这是给**自制文件选择器**用的，不弹系统对话框。
/// 只做一件事：列出一层的文件夹和文件，排序稳定；隐藏项（以 `.` 开头）跳过。
#[tauri::command]
pub fn list_path(path: Option<String>) -> Result<PathListing, String> {
    let Some(raw) = path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
    else {
        return Ok(PathListing {
            path: None,
            parent: None,
            entries: list_drives(),
        });
    };

    // 统一分隔符：用户从别处粘过来的路径常是正斜杠，显示出来会很别扭
    let raw = raw.replace('/', &std::path::MAIN_SEPARATOR.to_string());

    // 允许直接粘一个文件路径：自动跳到它所在目录
    let mut dir = std::path::PathBuf::from(&raw);
    if dir.is_file() {
        if let Some(parent) = dir.parent() {
            dir = parent.to_path_buf();
        }
    }
    if !dir.is_dir() {
        return Err(format!("目录不存在：{raw}"));
    }
    let dir = dir.as_path();

    let reader = std::fs::read_dir(dir).map_err(|e| format!("读取目录失败：{e}"))?;

    let mut dirs: Vec<PathEntry> = Vec::new();
    let mut files: Vec<PathEntry> = Vec::new();

    for entry in reader.flatten().take(4000) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path().to_string_lossy().to_string();

        if ft.is_dir() {
            dirs.push(PathEntry {
                name,
                path,
                is_dir: true,
            });
        } else if ft.is_file() {
            files.push(PathEntry {
                name,
                path,
                is_dir: false,
            });
        }
    }

    let key = |e: &PathEntry| e.name.to_lowercase();
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    dirs.extend(files);

    Ok(PathListing {
        path: Some(dir.to_string_lossy().to_string()),
        parent: dir
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .filter(|p| !p.is_empty()),
        entries: dirs,
    })
}

/* ------------------------------------------------------------ 封面编辑 */

/// 用本地图片当封面：把字节复制进应用自己的 `data/covers/`，
/// 文件名 = djb2(源路径)，前端 `coverSrc()` 无需区分来源。
#[tauri::command]
pub fn set_cover_from_file(
    app: AppHandle,
    db: State<'_, Db>,
    id: String,
    source: String,
) -> Result<Game, String> {
    let bytes = std::fs::read(&source).map_err(|e| format!("读取图片失败：{e}"))?;
    if !crate::db::cover_cache::is_image_bytes(&bytes) {
        return Err("这个文件不是图片（支持 jpg / png / webp / gif / bmp）".to_string());
    }

    let key = crate::db::scanner::path_hash(&source);
    crate::db::cover_cache::store(&app, &key, &bytes)?;

    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::set_cover(&conn, &id, Some(&source), true)
}

/// 用图片 URL 当封面：先下载到本地缓存，再写库。
/// 不下载的话断网就变裂图，而且封面 URL 常常带防盗链。
#[tauri::command]
pub async fn set_cover_from_url(
    app: AppHandle,
    id: String,
    url: String,
) -> Result<Game, String> {
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        return Err("图片地址不能为空".to_string());
    }

    let app2 = app.clone();
    let url2 = trimmed.clone();
    let downloaded = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let bytes = crate::scrape::fetch_bytes(&url2)?;
        if !crate::db::cover_cache::is_image_bytes(&bytes) {
            return Err("这个地址返回的不是图片".to_string());
        }
        let key = crate::db::scanner::path_hash(&url2);
        crate::db::cover_cache::store(&app2, &key, &bytes)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("下载线程异常：{e}"))?;
    downloaded?;

    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::set_cover(&conn, &id, Some(&trimmed), true)
}

/// 恢复成默认封面（清空 cover_url 并解锁，下次刮削还能重新填）
#[tauri::command]
pub fn reset_cover(db: State<'_, Db>, id: String) -> Result<Game, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::set_cover(&conn, &id, None, false)
}

/* ------------------------------------------------------------ 获取资料（多源） */

/// 搜候选。`source` 取 `vndb` / `bangumi` / `ymgal`。
#[tauri::command]
pub async fn source_search(
    source: String,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<crate::scrape::Candidate>, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Err("搜索词不能为空".to_string());
    }
    let src = source.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::scrape::search(&src, &q, limit.unwrap_or(10))
    })
    .await
    .map_err(|e| format!("搜索线程异常：{e}"))?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeReport {
    pub game: Game,
    pub updated_fields: Vec<String>,
    /// 因为被锁定而跳过的字段
    pub skipped_locked: Vec<String>,
    pub characters: usize,
    pub cover_saved: bool,
}

/// 一次"应用候选"的结果。`source_apply` 与「自动匹配」共用。
struct ApplyResult {
    game: Game,
    updated: Vec<String>,
    skipped: Vec<String>,
    characters: usize,
}

/// 把一条候选写进库里 —— 手动「获取资料」和扫描后的「自动匹配」共用这一份规则，
/// 不允许两套合并逻辑各自演化（否则同一个源在两条路径里会写出不同结果）。
///
/// 三条规矩（三个源共用同一套）：
/// 1. **锁定的字段一律不动**，并在报告里列出来；
/// 2. **声优不覆盖**：VNDB / 月幕不返回 CV，旧库从 Bangumi 补过 814 条。
///    按角色名把旧 CV 合并回新名单，否则一次获取就清空全部声优；
/// 3. 中文简介只写 `cn_description`，外文简介只写 `description`，互不覆盖。
fn apply_candidate_fields(
    conn: &rusqlite::Connection,
    id: &str,
    source: &str,
    source_id: &str,
    candidate: &crate::scrape::Candidate,
    chars: Vec<crate::models::Character>,
    cover_url: &str,
    cover_saved: bool,
) -> Result<ApplyResult, String> {
    let current = repo::get_one(conn, id)
        .map_err(db_err)?
        .ok_or_else(|| format!("找不到该游戏：{id}"))?;
    let locked: std::collections::HashSet<String> =
        current.locked_fields.iter().cloned().collect();

    let mut patch = GamePatch::default();
    let mut updated: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    macro_rules! put {
        ($field:ident, $camel:expr, $value:expr) => {{
            let v: String = $value;
            if v.trim().is_empty() {
                // 抓不到就保持原样，别把已有数据冲掉
            } else if locked.contains($camel) {
                skipped.push($camel.to_string());
            } else {
                patch.$field = Some(v);
                updated.push($camel.to_string());
            }
        }};
    }

    put!(title, "title", candidate.title.clone());
    put!(title_cn, "titleCn", candidate.title_cn.clone());
    put!(original_title, "originalTitle", candidate.alt_title.clone());
    put!(developer, "developer", candidate.developer.clone());
    put!(released, "released", candidate.released.clone());

    // 中文简介只进 cn_description；外文简介只在原来为空时补进 description
    put!(cn_description, "cnDescription", candidate.description_cn.clone());
    if current.description.trim().is_empty() {
        put!(description, "description", candidate.description.clone());
    }

    // 评分：各源都归一到 0~10；0 表示"没有评分"，不覆盖
    if candidate.rating > 0.0 {
        if locked.contains("rating") {
            skipped.push("rating".to_string());
        } else {
            patch.rating = Some(candidate.rating);
            patch.rating_source = Some(source.to_string());
            updated.push("rating".to_string());
        }
    }

    // 封面
    if !cover_url.is_empty() && cover_saved {
        if locked.contains("coverUrl") {
            skipped.push("coverUrl".to_string());
        } else {
            patch.cover_url = Some(cover_url.to_string());
            updated.push("coverUrl".to_string());
        }
    }

    // VNDB 的 id 单独存一列，方便下次直接定位
    if source == "vndb" {
        patch.vndb_id = Some(source_id.to_string());
    }

    let game = repo::update_game(conn, id, &patch, false)?;

    // 角色：按名字把旧声优合并回去，再整批替换
    let mut characters = 0usize;
    if !chars.is_empty() {
        let existing = repo::list_characters(conn, id).unwrap_or_default();
        let cv_by_name: std::collections::HashMap<String, (String, String)> = existing
            .iter()
            .map(|c| (c.name.clone(), (c.cv.clone(), c.cv_source.clone())))
            .collect();

        let mut merged = chars;
        for c in merged.iter_mut() {
            if c.cv.is_empty() {
                if let Some((cv, src)) = cv_by_name.get(&c.name) {
                    c.cv = cv.clone();
                    c.cv_source = src.clone();
                }
            }
        }
        characters = merged.len();
        repo::replace_characters(conn, id, &merged)?;
    }

    Ok(ApplyResult {
        game,
        updated,
        skipped,
        characters,
    })
}

/// 下载封面并写进本地缓存（同步版，只能跑在阻塞线程里）。
fn save_cover(app: &AppHandle, url: &str) -> bool {
    if url.trim().is_empty() {
        return false;
    }
    let Ok(bytes) = crate::scrape::fetch_bytes(url) else {
        return false;
    };
    if !crate::db::cover_cache::is_image_bytes(&bytes) {
        return false;
    }
    let key = crate::db::scanner::path_hash(url);
    crate::db::cover_cache::store(app, &key, &bytes).is_ok()
}

/// 异步命令里下载封面：丢进阻塞线程，别占着 async 运行时。
async fn download_cover(app: &AppHandle, url: &str) -> bool {
    if url.trim().is_empty() {
        return false;
    }
    let app2 = app.clone();
    let url2 = url.to_string();
    tauri::async_runtime::spawn_blocking(move || save_cover(&app2, &url2))
        .await
        .unwrap_or(false)
}

/// 把某个源上的一条记录应用到游戏（手动路径）。
#[tauri::command]
pub async fn source_apply(
    app: AppHandle,
    id: String,
    source: String,
    source_id: String,
) -> Result<ScrapeReport, String> {
    // ---------- 1. 取详情（不持锁） ----------
    let src = source.clone();
    let sid = source_id.clone();
    let (candidate, chars) = tauri::async_runtime::spawn_blocking(move || {
        crate::scrape::detail(&src, &sid)
    })
    .await
    .map_err(|e| format!("获取线程异常：{e}"))??;

    // ---------- 2. 下载封面（不持锁） ----------
    let cover_url = candidate.image_url.clone().unwrap_or_default();
    let cover_saved = download_cover(&app, &cover_url).await;

    // ---------- 3. 写库（持锁） ----------
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let applied = apply_candidate_fields(
        &conn, &id, &source, &source_id, &candidate, chars, &cover_url, cover_saved,
    )?;

    Ok(ScrapeReport {
        game: applied.game,
        updated_fields: applied.updated,
        skipped_locked: applied.skipped,
        characters: applied.characters,
        cover_saved,
    })
}

/* ------------------------------------------------------------ 自动匹配（扫描后） */

/// 自动匹配的写入门槛：归一化后**完全相等**才算命中（见 `scrape::title_score`）。
///
/// 为什么这么严：自动写入会一次改掉标题、会社、发售日、评分、封面、角色，
/// 认错一部就是一份脏数据，用户还得逐个字段改回来。宁可漏（留给手动），不可错。
const AUTO_MATCH_MIN_SCORE: f64 = 1.0;

/// 自动匹配依次尝试的源。
///
/// - `vndb`：字段最全，还带中文标题和角色；
/// - `ymgal`：中文圈条目，VNDB 没有的作品常能在这里找到；
/// - `bangumi` **不参与**：它的搜索是模糊的（搜「王様恋愛」第一条是「王様物語」），
///   不满足"完全相等"这个前提，硬接进来只会写错数据。
const AUTO_MATCH_SOURCES: [&str; 2] = ["vndb", "ymgal"];

/// `app_settings` 里"扫描后自动匹配"的开关
const AUTO_MATCH_KEY: &str = "scrape.autoMatchOnImport";

pub const AUTO_MATCH_PROGRESS_EVENT: &str = "auto-match-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMatchProgress {
    pub done: usize,
    pub total: usize,
    pub title: String,
    /// searching | matched | skipped | failed
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMatchHit {
    pub game_id: String,
    /// 库里原来的标题（多为文件夹名）
    pub title: String,
    pub source: String,
    pub source_id: String,
    /// 匹配到的条目标题
    pub matched_title: String,
    pub updated_fields: Vec<String>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMatchMiss {
    pub game_id: String,
    pub title: String,
    pub reason: String,
    /// 最接近但没达标的条目 —— 让用户知道"要不要手动看一下这个"
    pub closest: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMatchReport {
    pub matched: Vec<AutoMatchHit>,
    pub unmatched: Vec<AutoMatchMiss>,
    pub failed: Vec<AutoMatchMiss>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeSettings {
    /// 扫描完成后，自动为新入库的游戏匹配资料（默认开）
    pub auto_match: bool,
}

#[tauri::command]
pub fn get_scrape_settings(db: State<'_, Db>) -> Result<ScrapeSettings, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let raw = repo::get_setting(&conn, AUTO_MATCH_KEY).map_err(db_err)?;
    Ok(ScrapeSettings {
        // 没写过 = 默认开
        auto_match: raw.as_deref() != Some("0"),
    })
}

#[tauri::command]
pub fn set_scrape_settings(db: State<'_, Db>, auto_match: bool) -> Result<ScrapeSettings, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::set_setting(&conn, AUTO_MATCH_KEY, if auto_match { "1" } else { "0" }).map_err(db_err)?;
    Ok(ScrapeSettings { auto_match })
}

fn folder_name_of(game: &Game) -> String {
    std::path::Path::new(&game.folder_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn display_title(game: &Game) -> String {
    if !game.title_cn.trim().is_empty() {
        game.title_cn.clone()
    } else if !game.title.trim().is_empty() {
        game.title.clone()
    } else {
        folder_name_of(game)
    }
}

/// 拿什么去搜：原文名优先（VNDB 对日文原名最准），其次中文名，最后文件夹名。
fn match_query(game: &Game) -> String {
    let folder = folder_name_of(game);
    for candidate in [
        &game.original_title,
        &game.title_cn,
        &game.title,
        &folder,
    ] {
        if !candidate.trim().is_empty() {
            return candidate.trim().to_string();
        }
    }
    String::new()
}

enum MatchOutcome {
    Hit(Box<AutoMatchHit>),
    Miss(Box<AutoMatchMiss>),
}

fn emit_match_progress(app: &AppHandle, done: usize, total: usize, title: &str, status: &str) {
    let _ = app.emit(
        AUTO_MATCH_PROGRESS_EVENT,
        AutoMatchProgress {
            done,
            total,
            title: title.to_string(),
            status: status.to_string(),
        },
    );
}

/// 给一批游戏自动匹配资料。
///
/// 调用时机：扫描结束后，前端把 `insertedIds` 传进来。逐条处理，
/// 每条都会发一次 `auto-match-progress`，让界面能显示"正在匹配 3/18"。
#[tauri::command]
pub async fn auto_match_games(app: AppHandle, ids: Vec<String>) -> Result<AutoMatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || run_auto_match(&app, &ids))
        .await
        .map_err(|e| format!("匹配线程异常：{e}"))?
}

fn run_auto_match(app: &AppHandle, ids: &[String]) -> Result<AutoMatchReport, String> {
    let total = ids.len();
    let mut report = AutoMatchReport::default();

    for (index, id) in ids.iter().enumerate() {
        // 读一行只需要一瞬间：锁在这里拿、在这里放，绝不带进网络请求
        let game = {
            let db = app.state::<Db>();
            let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
            repo::get_one(&conn, id).map_err(db_err)?
        };
        let Some(game) = game else {
            continue;
        };

        let title = display_title(&game);
        emit_match_progress(app, index, total, &title, "searching");

        // 已经有人工修改过的条目不动：可能是用户刚改完，自动匹配不该覆盖他的判断
        if !game.locked_fields.is_empty() {
            emit_match_progress(app, index, total, &title, "skipped");
            report.unmatched.push(AutoMatchMiss {
                game_id: game.id.clone(),
                title,
                reason: "这条已经有人工修改，自动匹配不会覆盖".to_string(),
                closest: None,
            });
            continue;
        }

        match match_one_game(app, &game) {
            Ok(MatchOutcome::Hit(hit)) => {
                emit_match_progress(app, index, total, &title, "matched");
                report.matched.push(*hit);
            }
            Ok(MatchOutcome::Miss(miss)) => {
                emit_match_progress(app, index, total, &title, "skipped");
                report.unmatched.push(*miss);
            }
            Err(err) => {
                emit_match_progress(app, index, total, &title, "failed");
                report.failed.push(AutoMatchMiss {
                    game_id: game.id.clone(),
                    title,
                    reason: err,
                    closest: None,
                });
            }
        }
    }

    Ok(report)
}

fn match_one_game(app: &AppHandle, game: &Game) -> Result<MatchOutcome, String> {
    let query = match_query(game);
    if query.chars().count() < 2 {
        return Ok(MatchOutcome::Miss(Box::new(AutoMatchMiss {
            game_id: game.id.clone(),
            title: display_title(game),
            reason: "没有可用的标题".to_string(),
            closest: None,
        })));
    }

    let mut closest: Option<String> = None;
    let mut errors: Vec<String> = Vec::new();

    for source in AUTO_MATCH_SOURCES {
        let candidates = match crate::scrape::search(source, &query, 6) {
            Ok(rows) => rows,
            Err(err) => {
                errors.push(format!("{source} 搜索失败：{err}"));
                continue;
            }
        };

        let best = candidates
            .iter()
            .map(|c| (c, crate::scrape::title_score(&query, c)))
            .filter(|(_, score)| *score > 0.0)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let Some((candidate, score)) = best else {
            continue;
        };

        if score >= AUTO_MATCH_MIN_SCORE {
            // 详情 → 封面 → 写库。三步都在阻塞线程里，顺序执行。
            let (detail, chars) = crate::scrape::detail(source, &candidate.id)?;
            let cover_url = detail.image_url.clone().unwrap_or_default();
            let cover_saved = save_cover(app, &cover_url);

            let applied = {
                let db = app.state::<Db>();
                let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
                apply_candidate_fields(
                    &conn,
                    &game.id,
                    source,
                    &candidate.id,
                    &detail,
                    chars,
                    &cover_url,
                    cover_saved,
                )?
            };

            let matched_title = if !detail.title_cn.trim().is_empty() {
                detail.title_cn.clone()
            } else if !detail.title.trim().is_empty() {
                detail.title.clone()
            } else {
                detail.alt_title.clone()
            };

            // 顺手补中文简介与声优：VNDB 只有英文简介、没有声优，
            // 这一步失败不影响"已匹配"这个结果，所以忽略错误。
            let _ = run_enrich(app, &game.id, true);

            return Ok(MatchOutcome::Hit(Box::new(AutoMatchHit {
                game_id: game.id.clone(),
                title: display_title(game),
                source: source.to_string(),
                source_id: candidate.id.clone(),
                matched_title,
                updated_fields: applied.updated,
                score,
            })));
        }

        if closest.is_none() {
            closest = Some(format!("{}（{:.0}% 像）", candidate.title, score * 100.0));
        }
    }

    // 全都搜不动 = 网络问题，和"搜到了但不像"是两件事，分开报
    if closest.is_none() && !errors.is_empty() {
        return Err(errors.join("；"));
    }

    Ok(MatchOutcome::Miss(Box::new(AutoMatchMiss {
        game_id: game.id.clone(),
        title: display_title(game),
        reason: if closest.is_some() {
            "搜到了相似条目，但不够确定，留给你手动确认".to_string()
        } else {
            "没有搜到同名条目".to_string()
        },
        closest,
    })))
}

/* ------------------------------------------------------ 补全：中文简介 / 声优 */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichReport {
    pub game: Game,
    /// 中文简介来自哪个源（None = 没补到）
    pub description_source: Option<String>,
    /// 这次补上声优的角色数
    pub cv_filled: usize,
    pub characters: usize,
    pub notes: Vec<String>,
}

/// 补全一部游戏的中文简介与角色声优。
///
/// 这是"旧项目的相关功能"：VNDB 有角色但没有声优、简介是英文；
/// 中文简介和声优要从月幕 / Bangumi 补。两个源都按标题精确匹配，
/// 找不到就不动 —— 宁肯留着空，也不要写错角色。
#[tauri::command]
pub async fn enrich_game(app: AppHandle, id: String) -> Result<EnrichReport, String> {
    tauri::async_runtime::spawn_blocking(move || run_enrich(&app, &id, true))
        .await
        .map_err(|e| format!("补全线程异常：{e}"))?
}

/// 角色名归一化：只留字母/数字/假名/汉字，小写。
/// VNDB 的 "桜小路 ルナ" 和月幕的 "桜小路ルナ" 要能对上。
fn norm_character_name(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn merge_characters(
    existing: Vec<crate::models::Character>,
    incoming: Vec<crate::models::Character>,
    filled: &mut usize,
) -> Vec<crate::models::Character> {
    if existing.is_empty() {
        *filled = incoming.iter().filter(|c| !c.cv.trim().is_empty()).count();
        return incoming;
    }

    let mut by_name: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, c) in existing.iter().enumerate() {
        by_name.insert(norm_character_name(&c.name), i);
        for a in &c.aliases {
            by_name.insert(norm_character_name(a), i);
        }
    }

    let mut out = existing;
    for inc in incoming {
        let Some(&i) = by_name.get(&norm_character_name(&inc.name)) else {
            continue;
        };
        let target = &mut out[i];
        if target.cv.trim().is_empty() && !inc.cv.trim().is_empty() {
            target.cv = inc.cv;
            target.cv_source = inc.cv_source;
            *filled += 1;
        }
        if target.image_url.is_none() {
            target.image_url = inc.image_url;
        }
        for a in inc.aliases {
            if !target.aliases.contains(&a) {
                target.aliases.push(a);
            }
        }
    }
    out
}

/// 找这个游戏在某个源里的条目 id：先看 `game_sources`，没有再按标题精确搜。
fn resolve_source_id(app: &AppHandle, game: &Game, source: &str) -> Result<Option<String>, String> {
    {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        if let Some(sid) = repo::get_source_id(&conn, &game.id, source).map_err(db_err)? {
            if !sid.trim().is_empty() {
                return Ok(Some(sid));
            }
        }
    }

    let folder = folder_name_of(game);
    let mut queries: Vec<String> = Vec::new();
    for q in [&game.original_title, &game.title_cn, &game.title, &folder] {
        let q = q.trim();
        if !q.is_empty() && !queries.iter().any(|x| x == q) {
            queries.push(q.to_string());
        }
    }

    for q in queries {
        let rows = match crate::scrape::search(source, &q, 6) {
            Ok(rows) => rows,
            Err(_) => continue, // 这个源连不上就换下一个标题/源
        };
        let best = rows
            .iter()
            .map(|c| (c, crate::scrape::title_score(&q, c)))
            .filter(|(_, score)| *score >= 1.0)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((candidate, _)) = best {
            // 记住，下次补全/手动获取都不用再搜
            if let Ok(db) = Ok::<_, String>(app.state::<Db>()) {
                if let Ok(conn) = db.0.lock() {
                    let _ = repo::remember_source_id(&conn, &game.id, source, &candidate.id);
                }
            }
            return Ok(Some(candidate.id.clone()));
        }
    }

    Ok(None)
}

fn run_enrich(app: &AppHandle, id: &str, with_characters: bool) -> Result<EnrichReport, String> {
    let (game, existing) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        let game = repo::get_one(&conn, id)
            .map_err(db_err)?
            .ok_or_else(|| format!("找不到该游戏：{id}"))?;
        let chars = repo::list_characters(&conn, id).unwrap_or_default();
        (game, chars)
    };

    let locked: std::collections::HashSet<String> =
        game.locked_fields.iter().cloned().collect();
    let need_desc = game.cn_description.trim().is_empty() && !locked.contains("cnDescription");
    let need_cv = with_characters
        && (existing.is_empty() || existing.iter().any(|c| c.cv.trim().is_empty()));

    let mut notes: Vec<String> = Vec::new();
    let mut description: Option<(String, String)> = None;
    let mut incoming: Vec<crate::models::Character> = Vec::new();

    if need_desc || need_cv {
        for source in ["ymgal", "bangumi"] {
            if (!need_desc || description.is_some()) && (!need_cv || !incoming.is_empty()) {
                break;
            }

            let source_id = match resolve_source_id(app, &game, source) {
                Ok(Some(sid)) => sid,
                Ok(None) => {
                    notes.push(format!("{source} 上没有找到同名条目"));
                    continue;
                }
                Err(e) => {
                    notes.push(format!("{source} 搜索失败：{e}"));
                    continue;
                }
            };

            match crate::scrape::detail(source, &source_id) {
                Ok((candidate, returned)) => {
                    if need_desc && description.is_none() {
                        let cn = candidate.description_cn.trim();
                        if !cn.is_empty() {
                            description = Some((source.to_string(), cn.to_string()));
                        }
                    }

                    if need_cv && incoming.is_empty() {
                        if source == "ymgal" {
                            // 月幕的详情不带角色，要再按 gid 拉一次
                            match crate::scrape::ymgal::characters(&source_id) {
                                Ok(list) if !list.is_empty() => incoming = list,
                                Ok(_) => notes.push("月幕没有收录这部作品的角色".to_string()),
                                Err(e) => notes.push(format!("月幕角色读取失败：{e}")),
                            }
                        } else {
                            incoming = returned;
                        }
                    }
                }
                Err(e) => notes.push(format!("{source} 详情读取失败：{e}")),
            }
        }
    }

    let mut patch = GamePatch::default();
    if let Some((_, text)) = &description {
        patch.cn_description = Some(text.clone());
    }

    // 合并放在锁外：后面还要按角色立绘 URL 补下载，网络请求绝不能持着数据库锁
    let mut cv_filled = 0usize;
    let merged = merge_characters(existing.clone(), incoming, &mut cv_filled);
    let characters = merged.len();

    // 角色立绘：详情页只认本地缓存（`cover://`），没有缓存就只能显示首字占位。
    // 这里顺手补，已经有了就跳过，不会重复下载。
    for c in &merged {
        if let Some(url) = c.image_url.as_deref() {
            let key = crate::db::scanner::path_hash(url);
            if !crate::db::cover_cache::exists(app, &key) {
                let _ = save_cover(app, url);
            }
        }
    }

    let updated = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;

        // 只有"本来没角色"或"确实补到了声优"才整批替换；否则不碰现有列表
        if characters > 0 && (existing.is_empty() || cv_filled > 0) {
            repo::replace_characters(&conn, id, &merged)?;
        }

        if patch.cn_description.is_some() {
            repo::update_game(&conn, id, &patch, false)?
        } else {
            repo::get_one(&conn, id)
                .map_err(db_err)?
                .ok_or_else(|| format!("找不到该游戏：{id}"))?
        }
    };

    Ok(EnrichReport {
        game: updated,
        description_source: description.map(|(source, _)| source),
        cv_filled,
        characters,
        notes,
    })
}

/* ------------------------------------------------------ 统计体积（补旧数据） */

pub const SIZE_PROGRESS_EVENT: &str = "size-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeProgress {
    pub done: usize,
    pub total: usize,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeReport {
    /// 这次真正写了库的条数
    pub measured: usize,
    pub total_bytes: u64,
    /// 开始前有多少条没有体积
    pub missing_before: usize,
    /// 目录已经不存在（被改名/移动/删除）的条数，统计不了
    pub missing_dirs: usize,
}

/// 统计游戏目录的体积（递归），写回 `dir_size` / `file_count`。
///
/// 旧库没有体积列，211 部迁移过来全是 0；这个命令就是"补统计"。
/// `only_missing = true` 只补 0 的那些；false 则全部重算（挪过文件后刷新）。
#[tauri::command]
pub async fn measure_sizes(
    app: AppHandle,
    only_missing: bool,
) -> Result<SizeReport, String> {
    tauri::async_runtime::spawn_blocking(move || run_measure(&app, only_missing))
        .await
        .map_err(|e| format!("统计线程异常：{e}"))?
}

fn run_measure(app: &AppHandle, only_missing: bool) -> Result<SizeReport, String> {
    let games = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        repo::list_all_any(&conn).map_err(db_err)?
    };

    let missing_before = games.iter().filter(|g| g.dir_size <= 0).count();
    let targets: Vec<&Game> = games
        .iter()
        .filter(|g| !only_missing || g.dir_size <= 0)
        .collect();

    let total = targets.len();
    let mut measured = 0usize;
    let mut total_bytes = 0u64;
    let mut missing_dirs = 0usize;
    let mut last_emit = Instant::now() - std::time::Duration::from_millis(200);

    for (index, game) in targets.iter().enumerate() {
        let path = std::path::Path::new(&game.folder_path);
        if path.is_dir() {
            let (files, bytes) = scanner::measure_dir(path);
            if bytes > 0 {
                let db = app.state::<Db>();
                if let Ok(conn) = db.0.lock() {
                    let _ = repo::set_dir_size(&conn, &game.id, files, bytes as i64);
                }
                measured += 1;
                total_bytes += bytes;
            }
        } else {
            // 文件夹被改名/移动/删除了：统计不了，如实报数，别把它算成 0 字节
            missing_dirs += 1;
        }

        if last_emit.elapsed() >= std::time::Duration::from_millis(150)
            || index + 1 == total
        {
            last_emit = Instant::now();
            let _ = app.emit(
                SIZE_PROGRESS_EVENT,
                SizeProgress {
                    done: index + 1,
                    total,
                    title: display_title(game),
                },
            );
        }
    }

    Ok(SizeReport {
        measured,
        total_bytes,
        missing_before,
        missing_dirs,
    })
}

/* ------------------------------------------------------------ 封面候选 */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverOption {
    pub url: String,
    pub label: String,
    /// 主封面 / 包装正面 / 数字版 …
    pub kind: String,
    pub source: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverCandidates {
    pub vndb_id: String,
    /// 条目主标题，界面上告诉用户"这些图来自哪一条"
    pub title: String,
    pub options: Vec<CoverOption>,
}

/// 列出**当前已关联的 VNDB 条目**自带的全部封面。
///
/// 不再按标题重新搜索：搜索会混进同名的重制版、外传、甚至完全不相干的作品，
/// 用户点一张就换错了。已关联条目的 id 在库里（`games.vndb_id`），直接读。
///
/// 封面来源分两层：
/// 1. VN 自身的 `image`（主封面，界面上排第一）；
/// 2. 该 VN 每个发售版本（release）的 `images` —— 初回限定、通常版、数字版、
///    主机移植版各有各的包装图，这才是"一部作品不止一张封面"的真实出处。
#[tauri::command]
pub async fn cover_candidates(
    db: State<'_, Db>,
    game_id: String,
    limit: Option<u32>,
) -> Result<CoverCandidates, String> {
    // 锁必须在 await 之前释放：MutexGuard 不是 Send，跨 await 持有会编译不过
    let (vndb_id, fallback_title) = {
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        let game = repo::get_one(&conn, &game_id)
            .map_err(db_err)?
            .ok_or_else(|| format!("找不到该游戏：{game_id}"))?;
        (
            game.vndb_id.unwrap_or_default().trim().to_string(),
            if !game.title_cn.is_empty() {
                game.title_cn
            } else if !game.title.is_empty() {
                game.title
            } else {
                game.original_title
            },
        )
    };

    if vndb_id.is_empty() {
        return Err(
            "这部还没关联 VNDB 条目。先在「获取资料」里选一条 VNDB 结果并应用，\
             之后这里会直接列出该条目自带的全部封面。"
                .to_string(),
        );
    }

    let max = limit.unwrap_or(30).clamp(1, 60);

    tauri::async_runtime::spawn_blocking(move || -> Result<CoverCandidates, String> {
        // 主封面
        let vn = crate::scrape::vndb::fetch_vn(&vndb_id)?;
        let title = {
            let zh = crate::scrape::vndb::zh_title(&vn);
            if zh.is_empty() {
                let t = crate::scrape::vndb::value_str(&vn, "title");
                if t.is_empty() { fallback_title } else { t }
            } else {
                zh
            }
        };

        let mut options: Vec<CoverOption> = Vec::new();
        if let Some(url) = vn
            .get("image")
            .and_then(|i| i.get("url"))
            .and_then(|u| u.as_str())
            .filter(|u| !u.is_empty())
        {
            options.push(CoverOption {
                url: url.to_string(),
                label: format!("{title} · 主封面"),
                kind: "主封面".to_string(),
                source: "vndb".to_string(),
                id: vndb_id.clone(),
            });
        }

        // VNDB 限流 1 req/s：主封面和版本图之间歇一下
        crate::scrape::vndb::polite_pause();

        // 版本图拿不到不算失败 —— 主封面已经够用了
        let covers = crate::scrape::vndb::fetch_release_covers(&vndb_id, max).unwrap_or_default();
        for c in covers {
            if options.iter().any(|o| o.url == c.url) {
                continue;
            }
            options.push(CoverOption {
                url: c.url,
                label: c.label,
                kind: c.kind,
                source: "vndb".to_string(),
                id: vndb_id.clone(),
            });
        }

        options.truncate(max as usize);

        Ok(CoverCandidates {
            vndb_id,
            title,
            options,
        })
    })
    .await
    .map_err(|e| format!("查询线程异常：{e}"))?
}

/* ------------------------------------------------------------ 网络与代理 */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettings {
    /// 空串 = 自动（系统代理 / 环境变量）
    pub proxy: String,
}

/// 统一代理地址：没写协议就补 `http://`，只接受 HTTP(S) 代理。
///
/// 不做 SOCKS：reqwest 需要额外 feature 才支持，而且 Clash / V2Ray 的
/// 混合端口本身就是 HTTP 代理，够用。
fn normalize_proxy(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    if !(with_scheme.starts_with("http://") || with_scheme.starts_with("https://")) {
        return Err("目前只支持 HTTP 代理，例如 http://127.0.0.1:7890".to_string());
    }

    reqwest::Proxy::all(&with_scheme).map_err(|e| format!("代理地址不可用：{e}"))?;
    Ok(with_scheme)
}

#[tauri::command]
pub fn get_network_settings(db: State<'_, Db>) -> Result<NetworkSettings, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let proxy = repo::get_setting(&conn, crate::scrape::PROXY_KEY)
        .map_err(db_err)?
        .unwrap_or_default();
    Ok(NetworkSettings { proxy })
}

/// 保存代理。返回落库后的实际值（可能被补全了 `http://` 前缀）。
#[tauri::command]
pub fn set_network_settings(
    db: State<'_, Db>,
    proxy: String,
) -> Result<NetworkSettings, String> {
    let normalized = normalize_proxy(&proxy)?;
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::set_setting(&conn, crate::scrape::PROXY_KEY, &normalized).map_err(db_err)?;
    // 立即生效：不用重启应用
    crate::scrape::set_proxy(&normalized);
    Ok(NetworkSettings {
        proxy: normalized,
    })
}

/// 用给定代理（`None` = 当前生效设置）依次探测三个源。
///
/// 串行而不是并发：VNDB 本身限流，并发测会让结果互相干扰；
/// 三个源各 8 秒上限，最坏 24 秒，用户点一次能等。
#[tauri::command]
pub async fn test_network(proxy: Option<String>) -> Result<Vec<crate::scrape::probe::ProbeResult>, String> {
    let target = match proxy {
        Some(p) => Some(normalize_proxy(&p)?),
        None => None,
    };

    tauri::async_runtime::spawn_blocking(move || {
        let effective = match target {
            Some(p) => p,
            None => crate::scrape::proxy(),
        };
        let arg = if effective.is_empty() {
            None
        } else {
            Some(effective.as_str())
        };
        crate::scrape::probe::run(arg)
    })
    .await
    .map_err(|e| format!("测试线程异常：{e}"))
}

/* ------------------------------------------------------------ 补丁管理 */

use crate::models::PatchRecord;
use crate::patch::{self, PatchCandidate, PatchPreview, RevertReport};

/// 取一部游戏；拿不到就是"已被删/被移动"，让上层报中文
fn load_game_for_patch(app: &AppHandle, game_id: &str) -> Result<Game, String> {
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::get_one(&conn, game_id)
        .map_err(db_err)?
        .ok_or_else(|| "找不到这部游戏".to_string())
}

/// 这部游戏装过哪些补丁（含已还原的，界面要能区分）
#[tauri::command]
pub fn patch_list(db: State<'_, Db>, game_id: String) -> Result<Vec<PatchRecord>, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    repo::list_patches(&conn, &game_id).map_err(db_err)
}

/// 预览：算出这个补丁会新增/覆盖哪些文件，**不动硬盘**
#[tauri::command]
pub async fn patch_preview(
    app: AppHandle,
    game_id: String,
    source: String,
) -> Result<PatchPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let game = load_game_for_patch(&app, &game_id)?;
        patch::preview(&app, &game, &source)
    })
    .await
    .map_err(|e| format!("预览线程异常：{e}"))?
}

/// 应用：先备份再覆盖。`set_launch_exe` 是补丁里的 exe 相对路径（可选）
#[tauri::command]
pub async fn patch_apply(
    app: AppHandle,
    game_id: String,
    staging_id: String,
    name: String,
    set_launch_exe: Option<String>,
) -> Result<PatchRecord, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let game = load_game_for_patch(&app, &game_id)?;
        let db = app.state::<Db>();
        patch::apply(
            &app,
            &db,
            &game,
            &staging_id,
            &name,
            set_launch_exe.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("应用线程异常：{e}"))?
}

/// 还原：能恢复的恢复，新增的删掉，没备份的如实报告
#[tauri::command]
pub async fn patch_revert(app: AppHandle, patch_id: String) -> Result<RevertReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        patch::revert(&db, &patch_id)
    })
    .await
    .map_err(|e| format!("还原线程异常：{e}"))?
}

/// 删掉一条补丁记录（连带备份目录）—— 只删记录，不碰已经写进游戏目录的文件
#[tauri::command]
pub async fn patch_delete(app: AppHandle, patch_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;

        let record = repo::get_patch(&conn, &patch_id).map_err(db_err)?;
        repo::delete_patch(&conn, &patch_id).map_err(db_err)?;
        drop(conn);

        if let Some(rec) = record {
            if !rec.backup_dir.is_empty() {
                let _ = std::fs::remove_dir_all(&rec.backup_dir);
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("删除线程异常：{e}"))?
}

/// 扫描游戏目录，找出疑似补丁的子目录（未登记 / 已登记都返回）。
///
/// 走阻塞线程：大目录要递归两三层，同步命令会把 IPC 线程占住，界面点开对话框会顿一下。
#[tauri::command]
pub async fn patch_scan(app: AppHandle, game_id: String) -> Result<Vec<PatchCandidate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
        let game = repo::get_one(&conn, &game_id)
            .map_err(db_err)?
            .ok_or_else(|| "找不到这部游戏".to_string())?;
        let known: Vec<String> = repo::list_patches(&conn, &game_id)
            .map_err(db_err)?
            .into_iter()
            .map(|p| p.source_path)
            .collect();
        drop(conn);

        Ok(patch::scan(&game, &known))
    })
    .await
    .map_err(|e| format!("扫描线程异常：{e}"))?
}

/// 安装器型补丁：拍快照 + 启动安装器，返回会话 id
#[tauri::command]
pub fn patch_installer_begin(
    db: State<'_, Db>,
    game_id: String,
    exe: String,
) -> Result<String, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let game = repo::get_one(&conn, &game_id)
        .map_err(db_err)?
        .ok_or_else(|| "找不到这部游戏".to_string())?;
    drop(conn);

    patch::installer_begin(&game, &exe)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerReport {
    pub patch: PatchRecord,
    /// 检测到的变化文件数；0 表示安装器没写任何东西到游戏目录
    pub changed: usize,
}

/// 安装器结束：对比快照，把变化的文件记成可还原的补丁记录
#[tauri::command]
pub async fn patch_installer_finish(
    app: AppHandle,
    game_id: String,
    session_id: String,
    name: String,
) -> Result<InstallerReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let game = load_game_for_patch(&app, &game_id)?;
        let db = app.state::<Db>();
        let (patch, changed) =
            patch::installer_finish(&app, &db, &game, &session_id, &name)?;
        Ok(InstallerReport { patch, changed })
    })
    .await
    .map_err(|e| format!("安装记录线程异常：{e}"))?
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn lists_subfolders_before_files() {
        let dir = std::env::temp_dir().join("moeshelf-path-picker");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("z_sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();

        let listing = list_path(Some(dir.to_string_lossy().to_string())).expect("列目录失败");
        assert!(listing.path.is_some());
        assert!(listing.parent.is_some(), "临时目录应该有上级");

        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names.first(), Some(&"z_sub"), "文件夹排在文件前面");
        assert!(listing.entries[0].is_dir);
        assert_eq!(names.last(), Some(&"a.txt"));
    }

    #[test]
    fn no_path_lists_drives() {
        let listing = list_path(None).expect("列盘符失败");
        assert!(listing.path.is_none());
        assert!(listing.parent.is_none());
        // Windows 上至少有一个盘
        #[cfg(windows)]
        assert!(!listing.entries.is_empty(), "应该至少列出 C:");
        assert!(listing.entries.iter().all(|e| e.is_dir));
    }

    #[test]
    fn missing_dir_is_an_error() {
        let ghost = std::path::Path::new("Z://").join("definitely-not-here");
        assert!(list_path(Some(ghost.to_string_lossy().to_string())).is_err());
    }
}
