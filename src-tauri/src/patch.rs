//! 补丁管理。
//!
//! 三件事：**看清一个补丁会动哪些文件**、**动手前先备份**、**随时能退回去**。
//!
//! 支持的补丁形态：
//! - `.zip` 压缩包（纯 Rust 解压，兼容 GBK 中文文件名）
//! - 文件夹（已经解压好的补丁，最常见的形态）
//! - `.exe` 安装器（运行它，安装前后各拍一次目录快照，对比出改了什么）
//!
//! `.rar` / `.7z` 不直接支持：要读这两种格式，要么随包分发一个 7-Zip 的
//! 可执行文件，要么引一串 C 依赖。而补丁下载下来基本都会先解压一次，
//! 所以这两类在预览阶段就给明确引导（"解压后选文件夹"），而不是假装能处理。
//!
//! 路径安全：解压和写入都只接受目标目录内的相对路径（拒绝 `..` / 绝对路径），
//! 压缩包里的 `../../windows/system32` 这类条目会被直接丢弃。

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::AppHandle;

use crate::db::{repo, Db};
use crate::models::{Game, GamePatch, PatchFileRow, PatchRecord};

/// 单个文件超过这个大小就不备份。
///
/// Galgame 的汉化补丁经常替换几个 GB 的 `*.mpk`，全备份能把硬盘吃掉几十 GB。
/// 与其偷偷占满盘子，不如在预览里标红、让用户自己决定要不要为这个补丁牺牲可还原性。
const BACKUP_LIMIT: u64 = 200 * 1024 * 1024;

/// 预览会话的有效期。用户点开预览又关掉，暂存的解压目录要自己消失。
const SESSION_TTL: Duration = Duration::from_secs(2 * 60 * 60);

/// 目录里这些文件不参与补丁（系统垃圾 + 解压工具留下的说明文件）
const JUNK_FILES: &[&str] = &[".ds_store", "thumbs.db", "desktop.ini"];

/* ------------------------------------------------------------------ 数据结构 */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchFilePreview {
    /// 相对游戏目录，`/` 分隔
    pub rel_path: String,
    /// add | replace
    pub action: String,
    pub size: u64,
    /// 原文件会不会被备份（超限的大文件不会）
    pub backup: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPreview {
    /// 传给 `patch_apply` 的凭证，指向这次解压出来的临时目录
    pub staging_id: String,
    /// archive | folder | installer
    pub kind: String,
    pub name: String,
    pub source: String,
    pub total_files: usize,
    pub added: usize,
    pub replaced: usize,
    /// 被覆盖但超过备份上限的文件数
    pub unbacked: usize,
    pub total_bytes: u64,
    /// 包里的 exe（相对路径），用来提示"要不要设成主程序"
    pub exes: Vec<String>,
    /// 最多列 200 条：再多用户也看不完
    pub files: Vec<PatchFilePreview>,
    pub truncated: bool,
    /// 安装器没有文件清单，要走"运行 + 快照"
    pub installer: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertReport {
    /// 从备份恢复的文件数
    pub restored: usize,
    /// 删掉的新增文件数
    pub removed: usize,
    /// 因为没备份而无法还原的文件数
    pub skipped: usize,
    /// 还原后的状态：reverted | partial
    pub status: String,
}

/// 扫描游戏目录时发现、但还没登记过的补丁目录
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchCandidate {
    pub path: String,
    pub rel_path: String,
    pub name: String,
    pub files: usize,
    pub size: u64,
    /// 已经作为补丁记录存在（按路径判断）
    pub registered: bool,
}

/* ------------------------------------------------------------------ 会话 */

struct PreviewSession {
    root: PathBuf,
    kind: String,
    source: String,
    game_hash: String,
    /// 归档解压出来的临时目录（folder 类型没有，删除时不用清）
    staging: Option<PathBuf>,
}

struct InstallerSession {
    game_dir: PathBuf,
    game_hash: String,
    exe: PathBuf,
    before: HashMap<String, (u64, u64)>,
    created: Instant,
}

static SESSIONS: OnceLock<Mutex<HashMap<String, PreviewSession>>> = OnceLock::new();
static INSTALLERS: OnceLock<Mutex<HashMap<String, InstallerSession>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, PreviewSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn installers() -> &'static Mutex<HashMap<String, InstallerSession>> {
    INSTALLERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn take_session(id: &str) -> Result<PreviewSession, String> {
    let mut guard = sessions().lock().map_err(|_| "补丁会话被占用".to_string())?;
    guard
        .remove(id)
        .ok_or_else(|| "预览已过期，请重新选择补丁".to_string())
}

/* ------------------------------------------------------------------ 小工具 */

fn new_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{ms:x}{seq:02x}")
}

fn now_iso(conn: &rusqlite::Connection) -> String {
    conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now')", [], |row| {
        row.get(0)
    })
    .unwrap_or_default()
}

fn patches_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::db::data_dir(app)?.join("patches");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建补丁目录失败：{e}"))?;
    Ok(dir)
}

fn staging_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = patches_root(app)?.join("staging");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建暂存目录失败：{e}"))?;
    Ok(dir)
}

fn backup_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = patches_root(app)?.join("backup");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败：{e}"))?;
    Ok(dir)
}

/// 相对路径统一用 `/`：数据库里存的就是它，跨平台不会因为分隔符对不上
fn rel_string(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// 把数据库里的相对路径还原成真实路径，并拒绝任何越界条目
fn join_rel(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = base.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

fn make_writable(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perm = meta.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(path, perm);
        }
    }
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 目录里的所有文件 → `(相对路径, 字节数)`，按路径排序保证结果稳定
fn walk_files(base: &Path) -> Result<Vec<(String, u64)>, String> {
    let mut out = Vec::new();
    let mut stack = vec![base.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("读取目录失败 {}：{e}", dir.display()))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录项失败：{e}"))?;
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let lower = entry.file_name().to_string_lossy().to_lowercase();
            if JUNK_FILES.contains(&lower.as_str()) {
                continue;
            }

            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if let Ok(rel) = path.strip_prefix(base) {
                out.push((rel_string(rel), size));
            }
        }
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// 剥掉压缩包里多余的一层同名目录（`补丁/汉化补丁/xxx.exe` → `汉化补丁/xxx.exe`）。
///
/// 只剥"目录里只有一个子目录、没有任何文件"的层，最多三层。
/// 带说明文档的包不会被剥 —— 那种包本来就应该让用户看到完整结构。
fn strip_single_root(root: &Path) -> PathBuf {
    let mut current = root.to_path_buf();

    for _ in 0..3 {
        let Ok(entries) = std::fs::read_dir(&current) else {
            break;
        };

        let mut only_dir: Option<PathBuf> = None;
        let mut count = 0usize;
        for entry in entries.flatten() {
            count += 1;
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                only_dir = Some(entry.path());
            }
        }

        match (count, only_dir) {
            (1, Some(dir)) => current = dir,
            _ => break,
        }
    }

    current
}

/* ------------------------------------------------------------------ 解压 */

/// 解压 zip。文件名优先按 UTF-8 解析，失败则按 GBK —— 国内补丁包两种情况各占一半。
fn unzip_to(src: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(src).map_err(|e| format!("打开压缩包失败：{e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("这个 zip 打不开：{e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取压缩包第 {i} 项失败：{e}"))?;

        let raw = entry.name_raw().to_vec();
        let name = match std::str::from_utf8(&raw) {
            Ok(s) => s.to_string(),
            Err(_) => encoding_rs::GBK.decode(&raw).0.into_owned(),
        };

        let Some(out) = join_rel(dest, &name.replace('\\', "/")) else {
            // 越界条目（`../xxx`）直接跳过，不报错中断整个包
            continue;
        };

        if entry.is_dir() {
            std::fs::create_dir_all(&out)
                .map_err(|e| format!("创建目录失败 {}：{e}", out.display()))?;
            continue;
        }

        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录失败 {}：{e}", parent.display()))?;
        }

        let mut target =
            std::fs::File::create(&out).map_err(|e| format!("写入失败 {}：{e}", out.display()))?;
        io::copy(&mut entry, &mut target).map_err(|e| format!("解压写入失败：{e}"))?;
    }

    Ok(())
}

/// 清理过期的暂存目录（每次预览顺手做，不需要定时任务）
fn clean_expired(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let fresh = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|age| age < SESSION_TTL)
            .unwrap_or(true);
        if !fresh {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/* ------------------------------------------------------------------ 预览 */

pub fn preview(app: &AppHandle, game: &Game, source: &str) -> Result<PatchPreview, String> {
    let src = PathBuf::from(source.trim());
    if !src.exists() {
        return Err(format!("找不到这个路径：{}", src.display()));
    }

    let game_dir = PathBuf::from(&game.folder_path);
    if !game_dir.is_dir() {
        return Err("游戏目录不存在，可能被移动或改名了".to_string());
    }

    let base = staging_root(app)?;
    clean_expired(&base);

    let id = new_id();

    let (kind, root, installer, staging): (String, PathBuf, bool, Option<PathBuf>) =
        if src.is_dir() {
            ("folder".to_string(), strip_single_root(&src), false, None)
        } else {
            let ext = src
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            match ext.as_str() {
                "zip" => {
                    let dest = base.join(&id);
                    std::fs::create_dir_all(&dest)
                        .map_err(|e| format!("创建暂存目录失败：{e}"))?;
                    unzip_to(&src, &dest)?;
                    (
                        "archive".to_string(),
                        strip_single_root(&dest),
                        false,
                        Some(dest),
                    )
                }
                "exe" => ("installer".to_string(), src.clone(), true, None),
                "rar" | "7z" => {
                    return Err(
                        "这个格式需要先用解压软件解开（右键 → 解压到当前文件夹），\
                         然后在这里选择解压出来的文件夹。zip 可以直接选。"
                            .to_string(),
                    );
                }
                other => {
                    return Err(format!(
                        "不认识的补丁格式「.{other}」。可以选 zip、文件夹，或安装器 exe。"
                    ));
                }
            }
        };

    let default_name = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| game.title_cn.clone());
    let source_text = src.to_string_lossy().to_string();

    if installer {
        if let Ok(mut guard) = sessions().lock() {
            guard.insert(
                id.clone(),
                PreviewSession {
                    root: root.clone(),
                    kind: kind.clone(),
                    source: source_text.clone(),
                    game_hash: game.id.clone(),
                    staging: None,
                },
            );
        }

        return Ok(PatchPreview {
            staging_id: id,
            kind,
            name: default_name,
            source: source_text,
            total_files: 0,
            added: 0,
            replaced: 0,
            unbacked: 0,
            total_bytes: 0,
            exes: vec![src
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()],
            files: Vec::new(),
            truncated: false,
            installer: true,
        });
    }

    let entries = walk_files(&root)?;
    if entries.is_empty() {
        return Err("这个补丁里没有文件".to_string());
    }

    let mut files = Vec::with_capacity(entries.len().min(200));
    let mut added = 0usize;
    let mut replaced = 0usize;
    let mut unbacked = 0usize;
    let mut total_bytes = 0u64;
    let mut exes = Vec::new();

    for (rel, size) in &entries {
        let exists = join_rel(&game_dir, rel).map(|p| p.exists()).unwrap_or(false);
        let action = if exists { "replace" } else { "add" };

        if exists {
            replaced += 1;
        } else {
            added += 1;
        }
        total_bytes += size;

        // 覆盖 + 超过上限 = 备份不了，界面要标红
        let backup = !exists || *size <= BACKUP_LIMIT;
        if exists && !backup {
            unbacked += 1;
        }

        if exes.len() < 12 && rel.to_lowercase().ends_with(".exe") {
            exes.push(rel.clone());
        }

        if files.len() < 200 {
            files.push(PatchFilePreview {
                rel_path: rel.clone(),
                action: action.to_string(),
                size: *size,
                backup,
            });
        }
    }

    let truncated = entries.len() > files.len();

    if let Ok(mut guard) = sessions().lock() {
        guard.insert(
            id.clone(),
            PreviewSession {
                root,
                kind: kind.clone(),
                source: source_text.clone(),
                game_hash: game.id.clone(),
                staging,
            },
        );
    }

    Ok(PatchPreview {
        staging_id: id,
        kind,
        name: default_name,
        source: source_text,
        total_files: entries.len(),
        added,
        replaced,
        unbacked,
        total_bytes,
        exes,
        files,
        truncated,
        installer: false,
    })
}

/* ------------------------------------------------------------------ 应用 */

pub fn apply(
    app: &AppHandle,
    db: &Db,
    game: &Game,
    staging_id: &str,
    name: &str,
    set_launch_exe: Option<&str>,
) -> Result<PatchRecord, String> {
    let base = backup_root(app)?;
    apply_inner(db, game, staging_id, name, set_launch_exe, &base)
}

/// 应用的实际实现。备份根目录作为参数传进来，单元测试才能指向临时目录。
fn apply_inner(
    db: &Db,
    game: &Game,
    staging_id: &str,
    name: &str,
    set_launch_exe: Option<&str>,
    backup_base: &Path,
) -> Result<PatchRecord, String> {
    let session = take_session(staging_id)?;
    if session.game_hash != game.id {
        return Err("这次预览属于另一部游戏，请重新选择补丁".to_string());
    }

    let entries = walk_files(&session.root)?;
    if entries.is_empty() {
        return Err("这个补丁里没有文件".to_string());
    }

    let game_dir = PathBuf::from(&game.folder_path);
    let patch_id = new_id();
    let backup_dir = backup_base.join(&game.id).join(&patch_id);
    std::fs::create_dir_all(&backup_dir).map_err(|e| format!("创建备份目录失败：{e}"))?;

    // (相对路径, 动作, 是否备份成功, 备份里的相对路径) —— 失败时按它回滚
    let mut done: Vec<(String, String, bool)> = Vec::new();
    let mut rows: Vec<PatchFileRow> = Vec::new();

    let mut apply_result: Result<(), String> = Ok(());

    for (rel, size) in &entries {
        let Some(target) = join_rel(&game_dir, rel) else {
            continue;
        };
        let Some(source_file) = join_rel(&session.root, rel) else {
            continue;
        };

        let existed = target.exists();
        let action = if existed { "replace" } else { "add" };

        let mut backed = false;
        if existed && *size <= BACKUP_LIMIT {
            if let Some(backup_path) = join_rel(&backup_dir.join("files"), rel) {
                if let Some(parent) = backup_path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        apply_result = Err(format!("创建备份目录失败：{e}"));
                        break;
                    }
                }
                match std::fs::copy(&target, &backup_path) {
                    Ok(_) => backed = true,
                    Err(e) => {
                        apply_result = Err(format!("备份 {rel} 失败：{e}（已停止，未覆盖）"));
                        break;
                    }
                }
            }
        }

        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                apply_result = Err(format!("创建目录失败 {}：{e}", parent.display()));
                break;
            }
        }

        // 只读文件（很多老 galgame 的主程序是只读属性）直接覆盖会失败
        if existed {
            make_writable(&target);
        }

        if let Err(e) = std::fs::copy(&source_file, &target) {
            apply_result = Err(format!("写入 {rel} 失败：{e}"));
            break;
        }

        done.push((rel.clone(), action.to_string(), backed));
        rows.push(PatchFileRow {
            rel_path: rel.clone(),
            action: action.to_string(),
            size: *size as i64,
            backed_up: backed,
        });
    }

    // 半路出错：把已经写进去的按备份恢复，别留下"改了一半"的游戏目录
    if let Err(err) = apply_result {
        rollback(&game_dir, &backup_dir, &done);
        cleanup_session(session);
        return Err(format!("{err}（已回滚这一部分改动）"));
    }

    let added = rows.iter().filter(|r| r.action == "add").count() as i64;
    let replaced = rows.iter().filter(|r| r.action == "replace").count() as i64;
    let unbacked = rows
        .iter()
        .filter(|r| r.action == "replace" && !r.backed_up)
        .count() as i64;

    let mut conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let applied_at = now_iso(&conn);

    let record = PatchRecord {
        id: patch_id.clone(),
        game_hash: game.id.clone(),
        name: if name.trim().is_empty() {
            session
                .source
                .clone()
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("未命名补丁")
                .to_string()
        } else {
            name.trim().to_string()
        },
        kind: session.kind.clone(),
        source_path: session.source.clone(),
        status: "applied".to_string(),
        applied_at,
        reverted_at: None,
        backup_dir: backup_dir.to_string_lossy().to_string(),
        notes: String::new(),
        file_count: rows.len() as i64,
        added_count: added,
        replaced_count: replaced,
        unbacked_count: unbacked,
    };

    repo::insert_patch(&mut conn, &record, &rows)
        .map_err(|e| format!("写入补丁记录失败：{e}"))?;

    // 补丁里带了主程序：用户勾了就顺手改掉启动项（并锁定，重扫不会覆盖）
    if let Some(choice) = set_launch_exe.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(abs) = join_rel(&game_dir, choice) {
            let mut patch = GamePatch::default();
            patch.launch_exe = Some(abs.to_string_lossy().to_string());
            let _ = repo::update_game(&conn, &game.id, &patch, true);
        }
    }

    drop(conn);
    cleanup_session(session);

    Ok(record)
}

fn rollback(game_dir: &Path, backup_dir: &Path, done: &[(String, String, bool)]) {
    for (rel, action, backed) in done.iter().rev() {
        let Some(target) = join_rel(game_dir, rel) else {
            continue;
        };
        match (action.as_str(), *backed) {
            ("replace", true) => {
                if let Some(src) = join_rel(&backup_dir.join("files"), rel) {
                    make_writable(&target);
                    let _ = std::fs::copy(src, target);
                }
            }
            ("add", _) => {
                let _ = std::fs::remove_file(target);
            }
            _ => {}
        }
    }
}

fn cleanup_session(session: PreviewSession) {
    if let Some(dir) = session.staging {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/* ------------------------------------------------------------------ 还原 */

pub fn revert(db: &Db, patch_id: &str) -> Result<RevertReport, String> {
    let conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;

    let record = repo::get_patch(&conn, patch_id)
        .map_err(|e| format!("读取补丁记录失败：{e}"))?
        .ok_or_else(|| "找不到这条补丁记录".to_string())?;

    let game = repo::get_one(&conn, &record.game_hash)
        .map_err(|e| format!("读取游戏失败：{e}"))?
        .ok_or_else(|| "这部游戏已经不在库里了".to_string())?;

    let files = repo::list_patch_files(&conn, patch_id)
        .map_err(|e| format!("读取补丁明细失败：{e}"))?;

    let game_dir = PathBuf::from(&game.folder_path);
    let backup_dir = PathBuf::from(&record.backup_dir);

    let mut restored = 0usize;
    let mut removed = 0usize;
    let mut skipped = 0usize;

    for f in &files {
        let Some(target) = join_rel(&game_dir, &f.rel_path) else {
            skipped += 1;
            continue;
        };

        match f.action.as_str() {
            "replace" if f.backed_up => {
                let Some(src) = join_rel(&backup_dir.join("files"), &f.rel_path) else {
                    skipped += 1;
                    continue;
                };
                if !src.is_file() {
                    skipped += 1;
                    continue;
                }
                make_writable(&target);
                match std::fs::copy(&src, &target) {
                    Ok(_) => restored += 1,
                    Err(_) => skipped += 1,
                }
            }
            "replace" => skipped += 1,
            _ => {
                if target.is_file() {
                    make_writable(&target);
                    match std::fs::remove_file(&target) {
                        Ok(_) => removed += 1,
                        Err(_) => skipped += 1,
                    }
                }
            }
        }
    }

    let status = if skipped > 0 { "partial" } else { "reverted" };
    let at = now_iso(&conn);
    repo::set_patch_status(&conn, patch_id, status, Some(&at))
        .map_err(|e| format!("更新补丁状态失败：{e}"))?;

    Ok(RevertReport {
        restored,
        removed,
        skipped,
        status: status.to_string(),
    })
}

/* ------------------------------------------------------------------ 扫描 */

/// 目录名里出现这些词（中文，直接包含）就当疑似补丁
const PATCH_WORDS_CN: &[&str] = &[
    "补丁", "汉化", "漢化", "破解", "修正", "升级", "特典", "免cd", "免dvd", "免安装", "免安裝",
];

/// 英文词必须落在**边界**上。
///
/// 直接 `contains` 会出大笑话：真实库里 `ToHeart\Model`（1412 个文件的游戏数据目录）
/// 因为含 "mod" 被认成补丁目录。边界判断后 `model` / `modern` 都不会命中，
/// 而 `update_data` / `patch_v102` / 单独的 `dlc` 仍然能认出来。
const PATCH_WORDS_EN: &[&str] = &["patch", "crack", "update", "dlc", "append", "mod"];

fn looks_like_patch_dir(name: &str) -> bool {
    let lower = name.to_lowercase();

    if PATCH_WORDS_CN.iter().any(|w| lower.contains(w)) {
        return true;
    }

    let bytes = lower.as_bytes();
    PATCH_WORDS_EN.iter().any(|word| {
        for (at, _) in lower.match_indices(word) {
            let before_ok = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
            let after = at + word.len();
            let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        false
    })
}

pub fn scan(game: &Game, known_paths: &[String]) -> Vec<PatchCandidate> {
    let game_dir = PathBuf::from(&game.folder_path);
    if !game_dir.is_dir() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(game_dir.clone(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        if depth >= 2 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();

            if !looks_like_patch_dir(&name) {
                stack.push((path, depth + 1));
                continue;
            }

            // 命中就统计一下大小（只数文件，不递归下钻）
            let files = walk_files(&path).unwrap_or_default();
            if files.is_empty() {
                continue;
            }
            let size: u64 = files.iter().map(|(_, s)| *s).sum();
            let rel = path
                .strip_prefix(&game_dir)
                .map(rel_string)
                .unwrap_or_else(|_| name.clone());
            let abs = path.to_string_lossy().to_string();

            out.push(PatchCandidate {
                registered: known_paths.iter().any(|p| p == &abs),
                path: abs,
                rel_path: rel,
                name,
                files: files.len(),
                size,
            });
        }
    }

    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

/* ------------------------------------------------------------------ 安装器 */

fn snapshot(dir: &Path) -> Result<HashMap<String, (u64, u64)>, String> {
    let mut map = HashMap::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if let Ok(rel) = path.strip_prefix(dir) {
                map.insert(rel_string(rel), (meta.len(), mtime_ms(&meta)));
            }
        }
    }

    Ok(map)
}

/// 拍一张安装前快照，并以游戏目录为工作目录启动安装器。
pub fn installer_begin(game: &Game, exe: &str) -> Result<String, String> {
    let exe_path = PathBuf::from(exe.trim());
    if !exe_path.is_file() {
        return Err("找不到安装器".to_string());
    }

    let game_dir = PathBuf::from(&game.folder_path);
    if !game_dir.is_dir() {
        return Err("游戏目录不存在".to_string());
    }

    let before = snapshot(&game_dir)?;
    let id = new_id();

    std::process::Command::new(&exe_path)
        .current_dir(&game_dir)
        .spawn()
        .map_err(|e| format!("启动安装器失败：{e}"))?;

    if let Ok(mut guard) = installers().lock() {
        // 装完忘了点"安装完成"的会话不能一直留着：快照是内存里的一张大表
        guard.retain(|_, s| s.created.elapsed() < Duration::from_secs(6 * 3600));
        guard.insert(
            id.clone(),
            InstallerSession {
                game_dir,
                game_hash: game.id.clone(),
                exe: exe_path,
                before,
                created: Instant::now(),
            },
        );
    }

    Ok(id)
}

/// 用户点"安装完成"后对比快照，把变化的文件记成一条补丁记录（可还原）。
pub fn installer_finish(
    app: &AppHandle,
    db: &Db,
    game: &Game,
    session_id: &str,
    name: &str,
) -> Result<(PatchRecord, usize), String> {
    let session = {
        let mut guard = installers()
            .lock()
            .map_err(|_| "安装会话被占用".to_string())?;
        guard
            .remove(session_id)
            .ok_or_else(|| "这次安装已经结束或过期了".to_string())?
    };

    if session.game_hash != game.id {
        return Err("这次安装属于另一部游戏".to_string());
    }

    let after = snapshot(&session.game_dir)?;

    // 新增 / 大小变了 / 修改时间变了（安装器重写同名文件时大小常常不变）
    let mut changed: Vec<(String, String, u64)> = Vec::new();
    for (rel, (size, mtime)) in &after {
        match session.before.get(rel) {
            None => changed.push((rel.clone(), "add".to_string(), *size)),
            Some((old_size, old_mtime)) => {
                if size != old_size || mtime != old_mtime {
                    changed.push((rel.clone(), "replace".to_string(), *size));
                }
            }
        }
    }
    changed.sort_by(|a, b| a.0.cmp(&b.0));

    if changed.is_empty() {
        return Ok((
            PatchRecord {
                id: String::new(),
                game_hash: game.id.clone(),
                name: name.to_string(),
                kind: "installer".to_string(),
                source_path: session.exe.to_string_lossy().to_string(),
                status: "applied".to_string(),
                applied_at: String::new(),
                reverted_at: None,
                backup_dir: String::new(),
                notes: String::new(),
                file_count: 0,
                added_count: 0,
                replaced_count: 0,
                unbacked_count: 0,
            },
            0,
        ));
    }

    let patch_id = new_id();
    let backup_dir = backup_root(app)?.join(&game.id).join(&patch_id);
    std::fs::create_dir_all(&backup_dir).map_err(|e| format!("创建备份目录失败：{e}"))?;

    let mut rows = Vec::new();
    let mut unbacked = 0i64;

    for (rel, action, size) in &changed {
        let backed = if action == "replace" && *size <= BACKUP_LIMIT {
            match join_rel(&session.game_dir, rel)
                .zip(join_rel(&backup_dir.join("files"), rel))
            {
                Some((target, dst)) => {
                    if let Some(parent) = dst.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::copy(&target, &dst).is_ok()
                }
                None => false,
            }
        } else {
            false
        };

        if action == "replace" && !backed {
            unbacked += 1;
        }

        rows.push(PatchFileRow {
            rel_path: rel.clone(),
            action: action.clone(),
            size: *size as i64,
            backed_up: backed,
        });
    }

    let added = rows.iter().filter(|r| r.action == "add").count() as i64;
    let replaced = rows.iter().filter(|r| r.action == "replace").count() as i64;

    let mut conn = db.0.lock().map_err(|_| "数据库连接被占用".to_string())?;
    let applied_at = now_iso(&conn);

    let record = PatchRecord {
        id: patch_id,
        game_hash: game.id.clone(),
        name: if name.trim().is_empty() {
            session
                .exe
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "安装器补丁".to_string())
        } else {
            name.trim().to_string()
        },
        kind: "installer".to_string(),
        source_path: session.exe.to_string_lossy().to_string(),
        status: "applied".to_string(),
        applied_at,
        reverted_at: None,
        backup_dir: backup_dir.to_string_lossy().to_string(),
        notes: "安装器运行，按安装前后对比记录".to_string(),
        file_count: rows.len() as i64,
        added_count: added,
        replaced_count: replaced,
        unbacked_count: unbacked,
    };

    repo::insert_patch(&mut conn, &record, &rows)
        .map_err(|e| format!("写入补丁记录失败：{e}"))?;

    let count = rows.len();
    Ok((record, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("moeshelf-patch-{tag}-{}", new_id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_game(id: &str, folder: &Path) -> Game {
        Game {
            id: id.to_string(),
            path_hash: id.to_string(),
            title: "Test".to_string(),
            title_cn: "测试".to_string(),
            original_title: String::new(),
            developer: String::new(),
            released: String::new(),
            engine: "unknown".to_string(),
            platform: "windows".to_string(),
            tags: Vec::new(),
            rating: 0.0,
            rating_source: String::new(),
            description: String::new(),
            cn_description: String::new(),
            cover_url: None,
            launch_exe: String::new(),
            folder_path: folder.to_string_lossy().to_string(),
            file_count: 0,
            dir_size: 0,
            match_score: 0.0,
            completed: false,
            first_seen_at: String::new(),
            locked_fields: Vec::new(),
            vndb_id: None,
        }
    }

    #[test]
    fn rejects_path_escape() {
        let base = Path::new("C:/games/x");
        assert!(join_rel(base, "../evil").is_none(), "上跳目录必须拒绝");
        assert!(join_rel(base, "a/../../evil").is_none());
        assert!(join_rel(base, "C:/windows").is_none(), "绝对路径必须拒绝");
        assert_eq!(
            join_rel(base, "sub/a.txt").unwrap(),
            base.join("sub").join("a.txt")
        );
        assert_eq!(join_rel(base, "./a.txt").unwrap(), base.join("a.txt"));
    }

    #[test]
    fn strips_single_wrapper_dir_only() {
        let one = tmp_dir("strip-one");
        let inner = one.join("汉化补丁");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("game.exe"), b"x").unwrap();
        assert_eq!(strip_single_root(&one), inner);

        let mixed = tmp_dir("strip-mixed");
        std::fs::create_dir_all(mixed.join("sub")).unwrap();
        std::fs::write(mixed.join("readme.txt"), b"x").unwrap();
        assert_eq!(strip_single_root(&mixed), mixed, "带说明文件的包不该被剥壳");
    }

    #[test]
    fn walk_skips_junk_files() {
        let dir = tmp_dir("walk");
        std::fs::write(dir.join("a.txt"), b"1").unwrap();
        std::fs::write(dir.join("Thumbs.db"), b"2").unwrap();
        std::fs::write(dir.join(".DS_Store"), b"3").unwrap();

        let list = walk_files(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "a.txt");
    }

    /// 端到端：覆盖 + 新增 → 还原后原文件回来、新增文件消失
    #[test]
    fn apply_then_revert_restores_files() {
        let root = tmp_dir("roundtrip");
        let game_dir = root.join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("main.exe"), b"old-exe").unwrap();
        std::fs::write(game_dir.join("keep.txt"), b"untouched").unwrap();

        let patch_dir = root.join("patch");
        std::fs::create_dir_all(&patch_dir).unwrap();
        std::fs::write(patch_dir.join("main.exe"), b"new-exe").unwrap();
        std::fs::write(patch_dir.join("extra.dat"), b"added").unwrap();

        let conn = crate::db::open_at(&root.join("test.db")).unwrap();
        conn.execute(
            "INSERT INTO games (id, path_hash, folder_path) VALUES ('g1','h1',?1)",
            [game_dir.to_string_lossy().to_string()],
        )
        .unwrap();

        let game = fake_game("g1", &game_dir);
        let db = Db(Mutex::new(conn));

        let staging = new_id();
        sessions().lock().unwrap().insert(
            staging.clone(),
            PreviewSession {
                root: patch_dir.clone(),
                kind: "folder".to_string(),
                source: patch_dir.to_string_lossy().to_string(),
                game_hash: "g1".to_string(),
                staging: None,
            },
        );

        let rec = apply_inner(
            &db,
            &game,
            &staging,
            "测试补丁",
            Some("main.exe"),
            &root.join("backup"),
        )
        .expect("应用应当成功");

        assert_eq!(rec.added_count, 1);
        assert_eq!(rec.replaced_count, 1);
        assert_eq!(rec.unbacked_count, 0);
        assert_eq!(std::fs::read(game_dir.join("main.exe")).unwrap(), b"new-exe");
        assert!(game_dir.join("extra.dat").exists(), "新增文件应当写入");

        // 备份里应该留着一份旧主程序
        let backed = PathBuf::from(&rec.backup_dir).join("files").join("main.exe");
        assert_eq!(std::fs::read(backed).unwrap(), b"old-exe");

        // 启动项被指到补丁里的主程序
        {
            let guard = db.0.lock().unwrap();
            let fresh = crate::db::repo::get_one(&guard, "g1").unwrap().unwrap();
            assert!(fresh.launch_exe.ends_with("main.exe"), "启动项应指向补丁的主程序");
        }

        let report = revert(&db, &rec.id).expect("还原应当成功");
        assert_eq!(report.status, "reverted");
        assert_eq!(report.restored, 1);
        assert_eq!(report.removed, 1);
        assert_eq!(report.skipped, 0);
        assert_eq!(std::fs::read(game_dir.join("main.exe")).unwrap(), b"old-exe");
        assert!(!game_dir.join("extra.dat").exists());
        assert_eq!(
            std::fs::read(game_dir.join("keep.txt")).unwrap(),
            b"untouched",
            "没被补丁碰过的文件不能动"
        );
    }

    #[test]
    fn scan_finds_patch_like_dirs() {
        let root = tmp_dir("scan");
        let game_dir = root.join("game");
        std::fs::create_dir_all(game_dir.join("补丁").join("v1.02")).unwrap();
        std::fs::write(game_dir.join("补丁").join("v1.02").join("game.exe"), b"x").unwrap();
        std::fs::create_dir_all(game_dir.join("movie")).unwrap();
        std::fs::write(game_dir.join("movie").join("op.mp4"), b"x").unwrap();

        let game = fake_game("g2", &game_dir);
        let found = scan(&game, &[]);
        assert_eq!(found.len(), 1, "只应认出「补丁」目录");
        assert_eq!(found[0].rel_path, "补丁");
        assert_eq!(found[0].files, 1);
    }

    #[test]
    fn patch_word_needs_boundary() {
        // 真实误报：ToHeart\Model 是游戏数据目录，不能因为含 "mod" 就被认成补丁
        assert!(!looks_like_patch_dir("Model"));
        assert!(!looks_like_patch_dir("modern_ui"));
        assert!(!looks_like_patch_dir("patchwm"), "patch 后面紧跟字母不算");

        assert!(looks_like_patch_dir("DLC"));
        assert!(looks_like_patch_dir("update_data"));
        assert!(looks_like_patch_dir("patch_v102"));
        assert!(looks_like_patch_dir("汉化补丁"));
        assert!(looks_like_patch_dir("免CD补丁"));
        assert!(looks_like_patch_dir("v1.02 修正"));
    }
}
