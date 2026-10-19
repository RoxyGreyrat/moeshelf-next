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
}

#[tauri::command]
pub fn get_game_detail(db: State<'_, Db>, id: String) -> Result<GameDetail, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let game = repo::get_one(&conn, &id)
        .map_err(db_err)?
        .ok_or_else(|| format!("找不到该游戏：{id}"))?;
    let characters = repo::list_characters(&conn, &id).map_err(db_err)?;
    Ok(GameDetail { game, characters })
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

/// 把某个源上的一条记录应用到游戏。
///
/// 三条规矩（三个源共用同一套）：
/// 1. **锁定的字段一律不动**，并在报告里列出来；
/// 2. **声优不覆盖**：VNDB / 月幕不返回 CV，旧库从 Bangumi 补过 814 条。
///    按角色名把旧 CV 合并回新名单，否则一次获取就清空全部声优；
/// 3. 中文简介只写 `cn_description`，外文简介只写 `description`，互不覆盖。
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
    let mut cover_saved = false;
    if !cover_url.is_empty() {
        let app2 = app.clone();
        let url2 = cover_url.clone();
        cover_saved = tauri::async_runtime::spawn_blocking(move || -> bool {
            let Ok(bytes) = crate::scrape::fetch_bytes(&url2) else {
                return false;
            };
            if !crate::db::cover_cache::is_image_bytes(&bytes) {
                return false;
            }
            let key = crate::db::scanner::path_hash(&url2);
            crate::db::cover_cache::store(&app2, &key, &bytes).is_ok()
        })
        .await
        .unwrap_or(false);
    }

    // ---------- 3. 写库（持锁） ----------
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;

    let current = repo::get_one(&conn, &id)
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
            patch.rating_source = Some(source.clone());
            updated.push("rating".to_string());
        }
    }

    // 封面
    if !cover_url.is_empty() && cover_saved {
        if locked.contains("coverUrl") {
            skipped.push("coverUrl".to_string());
        } else {
            patch.cover_url = Some(cover_url);
            updated.push("coverUrl".to_string());
        }
    }

    // VNDB 的 id 单独存一列，方便下次直接定位
    if source == "vndb" {
        patch.vndb_id = Some(source_id.clone());
    }

    let game = repo::update_game(&conn, &id, &patch, false)?;

    // 角色：按名字把旧声优合并回去，再整批替换
    let mut characters = 0usize;
    if !chars.is_empty() {
        let existing = repo::list_characters(&conn, &id).unwrap_or_default();
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
        repo::replace_characters(&conn, &id, &merged)?;
    }

    Ok(ScrapeReport {
        game,
        updated_fields: updated,
        skipped_locked: skipped,
        characters,
        cover_saved,
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
