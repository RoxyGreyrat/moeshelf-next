//! 目录扫描器（C 阶段）：从一个根目录递归找出"游戏目录"，产出可直接入库的条目。
//!
//! 规则移植自旧版 MoeShelf 的 `src/lib/scan.ts`（那套规则是踩了两年真机反馈磨出来的），
//! 但把 Node 的异步 fs 换成同步 Rust —— 扫描是纯 IO 活，跑在 `spawn_blocking` 线程里，
//! 不需要把 async 传染给整条调用链。
//!
//! ## 三层判定（缺一层就会大量误判）
//!
//! 1. **打分**：按扩展名权重累加（`.xp3` 6 分、`.exe` 4 分…），命中引擎特征再 +4，数据 ≥5MB 再 +2。
//! 2. **实锤**：光有分数**不算**游戏。必须有"另有实锤"之一 ——
//!    真正的数据文件 / 数据量 ≥5MB / 子目录里藏着 ≥5MB 数据 / 命中引擎特征。
//!    少了这一层，`foo.exe + helper.dll + 说明.txt` 这种工具目录会被收成游戏。
//! 3. **容器兜底**：主程序在子目录时（`Game/bin/game.exe` + `Game/data/data.xp3`），
//!    把**父目录**认成游戏。否则会被拆成 `bin`（有 exe）和 `data`（有 xp3）两条假条目。
//!
//! ## 为什么 path_hash 必须一模一样
//!
//! `path_hash = djb2(文件夹全路径)`，而且**按 UTF-16 码元**迭代（JS 的 `charCodeAt` 语义）。
//! 这个值同时是主键和封面缓存的钥匙。算错一位，扫描就会把已经迁移进来的 211 部
//! 全部当成新游戏再插一遍。

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::engine_probe;

/// 递归深度上限。Galgame 的目录结构很少超过 4 层。
const MAX_DEPTH: usize = 4;
/// 单个游戏目录里最多收集多少个可启动候选
const MAX_EXES: usize = 60;
/// 最多返回几个启动候选给界面
const MAX_CANDIDATES: usize = 10;
/// 统计体积时最多数多少个文件（防止有人把根目录指到盘符，把整块盘翻一遍）
const STATS_MAX_FILES: usize = 300_000;
/// "数据够大"的阈值：5 MB
const BIG_DATA: u64 = 5 * 1024 * 1024;
/// "主程序够大"的阈值：20 MB
const BIG_EXE: u64 = 20 * 1024 * 1024;
/// 进度事件节流：至少隔这么久才发一次，避免上千个目录把 IPC 打爆
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/* ------------------------------------------------------------------ 路径哈希 */

/// 旧版 `pathHashOf`：djb2，**按 UTF-16 码元**迭代。
///
/// 专门提一句：JS 的 `charCodeAt` 拿到的是码元而不是码点。中日文在 BMP 内两者一致，
/// 但遇到 emoji / 扩展区汉字就会分叉。这里必须和旧版逐位相同，见文件头说明。
pub fn path_hash(p: &str) -> String {
    let mut h: u32 = 5381;
    for unit in p.encode_utf16() {
        h = h.wrapping_mul(33).wrapping_add(unit as u32);
    }
    format!("{h:08x}")
}

/* -------------------------------------------------------------- 规则常量表 */

/// 扫描时直接跳过的系统/工具目录
///
/// 后 5 项是**对着真机结果补的**：第一次扫 `E:\galgame` 时，
/// `临时文件`（3824 个文件 + 启动.bat）和 `python`（python.exe）被当成了游戏。
/// 这两个名字在任何游戏库里都不可能是作品名，直接硬跳过最省事。
const SKIP_DIR_NAMES: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "windows",
    "program files",
    "program files (x86)",
    "node_modules",
    ".git",
    ".svn",
    "msocache",
    "perflogs",
    "recovery",
    "config.msi",
    // ↓ 真机扫描结果反推
    "临时文件",
    "temp",
    "tmp",
    "python",
    "python3",
    "__pycache__",
];

/// 补丁/备份类目录：**只用来排除启动候选**，不会阻止下钻。
/// 注意只匹配目录名 —— 有些游戏主程序本身就叫 `xxx_Patch.exe`（汉化直接改了主程序）。
const PATCHY_DIR_KW: &[&str] = &[
    "补丁", "汉化补丁", "patch", "crack", "破解", "备份", "backup", "bak", "update", "升级",
];

/// 工具/安装类 exe 的**精确名**（去掉扩展名后比较）
const JUNK_EXE_NAMES: &[&str] = &[
    "setup", "setup1", "setup2", "setup3", "setup_x86", "setup_x64", "unins000", "unins001",
    "unins002", "uninstall", "uninst", "inst", "install", "installer", "update", "updater",
    "updata", "patch", "patcher", "config", "configure", "cfg", "autorun", "readme", "eula",
    "license", "reg", "register", "regist", "vcredist", "vcredist_x86", "vcredist_x64",
    "vc_redist", "vc_redist.x86", "vc_redist.x64", "dxsetup", "dxwebsetup", "directx",
    "dotnetfx", "dotnetfx35", "dotnetfx40", "dotnet", "redist", "gxsetup", "crashreporter",
    "sendto", "msvcp", "msvcr", "d3dx9", "d3dx10", "unarc", "unrar", "7z", "7za", "winrar",
    "notepad", "cmd", "explorer", "安装", "卸载", "更新", "补丁", "说明", "常见问题", "注册表",
    "0", "1", "2",
];

/// 工具/运行库/修复类 exe 的**名字特征**（子串匹配）。
///
/// 为什么精确名不够：真机上出现过
/// `常用运行库\微软常用运行库合集（32位+64位）.exe`（31 MB，命中"主程序 ≥20MB"）
/// 和 `DirectX修复\DirectX Repair.exe`（配合同目录的 `Data\` 命中"有数据目录"），
/// 两条都被当成游戏，各多出一条假条目。
const TOOL_EXE_KW: &[&str] = &[
    "运行库", "运行环境", "环境包", "驱动", "修复", "repair", "directx", "dxsetup", "vcredist",
    "dotnet", "netfx", "redist", "installer", "setup", "unins", "卸载", "安装程序", "绿色版",
    "工具箱", "toolbox", "汉化工具", "解包", "解压", "portable",
];

/// 数据/资源类目录名：用来给"另有实锤"里那条弱信号 `hasDataDir`
const DATA_DIR_NAMES: &[&str] = &[
    "data", "savedata", "save_data", "save", "bgm", "bgs", "se", "voice", "movie", "movie2",
    "op", "ed", "system", "sys", "script", "scenario", "scenario2", "cg", "graph", "graphics",
    "image", "images", "sound", "sounds", "resource", "resources", "font", "fonts", "dll",
    "common", "patch", "update", "readme", "manual", "doc", "docs", "config", "setting",
    "settings", "log", "logs", "cache", "temp", "tmp", "debug", "screenshot", "capture", "icon",
    "icons", "text", "bin", "lib", "libs", "ext", "extensions", "plugin", "plugins", "mod",
    "mods", "tool", "tools", "backup", "存档", "语音", "音乐", "背景", "立绘", "头像", "补丁",
    "说明", "文档", "临时", "备份", "字体", "图片", "cg集", "演出", "效果", "脚本", "资源",
    "数据", "dat", "pdt", "pac", "arc",
];

/// 目录名像工具/补丁目录 —— 只作为"要不要继续下钻"的依据，不再无条件跳过
/// （否则真名叫 `patch` / `备份` 的游戏会被误杀）
const TOOL_LIKE_INCLUDES: &[&str] = &[
    "galtools", "解压", "解包", "提取工具", "汉化补丁", "备份", "运行库", "运行环境", "修复",
    "临时", "归档",
];
const TOOL_LIKE_EXACT: &[&str] = &[
    "tools", "tool", "backup", "补丁", "patch", "crack", "破解", "directx", "运行库",
    "vcredist",
];

/// 分卷目录名（`disc1/game.exe` + `disc2/game.exe`）
const PART_DIR_KW: &[&str] = &[
    "disc", "disk", "cd", "dvd", "part", "vol", "volume", "上卷", "下卷", "前篇", "后篇",
    "上篇", "下篇", "资料盘",
];

/* ------------------------------------------------------------ 基础名/后缀 */

/// `game.exe` → (`".exe"`, `"game"`)；没有后缀或点开头的返回 `("", name)`
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[i..], &name[..i]),
        _ => ("", name),
    }
}

fn is_junk_exe(base: &str) -> bool {
    if JUNK_EXE_NAMES.contains(&base) {
        return true;
    }
    TOOL_EXE_KW.iter().any(|kw| base.contains(kw))
}

fn is_tool_like_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    TOOL_LIKE_EXACT.contains(&lower.as_str())
        || TOOL_LIKE_INCLUDES.iter().any(|kw| lower.contains(kw))
}

fn is_part_dir(dir: &str) -> bool {
    // 只看最后一段目录名
    let last = dir.rsplit('/').next().unwrap_or(dir);
    PART_DIR_KW.iter().any(|kw| last.contains(kw))
}

fn is_skip_dir(lower: &str) -> bool {
    SKIP_DIR_NAMES.contains(&lower)
}

fn is_patchy_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    PATCHY_DIR_KW.iter().any(|kw| lower.contains(kw))
}

/// 扩展名权重 —— 移植自旧版 `extWeight`，数值本身没有意义，**相对大小**才是规则：
/// `.xp3`(6) 是 KiriKiri 独有容器；`.exe`(4) 也算分但只有 4；`.dat/.bin/.dll/.txt`(1)
/// 属于"谁都有"的弱信号。
fn ext_weight(ext: &str, base: &str) -> i64 {
    match ext {
        ".xp3" => 6,
        ".nsa" | ".noa" | ".nmc" | ".cvd" => 5,
        ".exe" => {
            if is_junk_exe(base) {
                1
            } else {
                4
            }
        }
        ".iso" | ".img" | ".mdf" | ".mds" => 4,
        ".rpa" | ".rpi" => 6,
        ".rvdata2" | ".rvdata" | ".rxdata" | ".ldb" | ".lmu" | ".wolfsave" | ".int"
        | ".majiro" | ".assets" | ".unity3d" => 4,
        ".paz" | ".hfa" => 3,
        ".lcf" | ".wolfdata" | ".stx" | ".maj" | ".rpy" | ".rpyc" | ".ress" | ".bundle"
        | ".nsc" | ".obb" => 3,
        ".arc" | ".pak" | ".ks" | ".tjs" | ".pkd" | ".pfs" | ".s3r" | ".ypf" | ".mpk"
        | ".pck" | ".pac" | ".ald" | ".g00" | ".sdc" | ".xfl" | ".ns2" | ".grp" | ".acr" => 2,
        ".dat2" => 2,
        ".dat" | ".pdt" | ".bin" | ".dll" | ".txt" => 1,
        _ => 0,
    }
}

/* ---------------------------------------------------------------- 目录快照 */

#[derive(Default)]
struct DirFacts {
    /// 直接子文件：(小写名, 字节数)
    files: Vec<(String, u64)>,
    /// 直接子目录（已排除隐藏目录），用于继续下钻
    subdirs: Vec<PathBuf>,
    /// 直接子目录名（小写），用于 `hasDataDir` 判定
    subdir_names: Vec<String>,
}

fn read_facts(dir: &Path) -> Option<DirFacts> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut facts = DirFacts::default();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        // 不跟随符号链接：Windows 上有目录联接（junction），跟下去可能成环
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if lower.starts_with('.') {
                continue;
            }
            facts.subdir_names.push(lower);
            facts.subdirs.push(entry.path());
        } else if ft.is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            facts.files.push((lower, size));
        }
    }

    Some(facts)
}

/* ------------------------------------------------------------ 引擎特征（打分用） */

/// 旧版 `ENGINE_SIGNATURES`：只看**顶层**文件名，命中一次就给 +4 分。
///
/// 这里只负责"打分时是否加分"，最终写进库的引擎由 `engine_probe`（会看两层目录、
/// 还会读主程序特征串）决定 —— 那个更准，但这个更便宜。
fn engine_signature(facts: &DirFacts) -> Option<&'static str> {
    let names: Vec<&str> = facts.files.iter().map(|(n, _)| n.as_str()).collect();
    let has = |pred: &dyn Fn(&str) -> bool| names.iter().any(|n| pred(n));
    let any_ext = |ext: &str| names.iter().any(|n| n.ends_with(ext));

    // 顺序即优先级，与旧版一致
    if has(&|n| n.starts_with("avg32") && n.ends_with(".dll"))
        || (any_ext(".dat") && any_ext(".pdt"))
    {
        return Some("avg32");
    }
    if has(&|n| n.starts_with("reallivedata"))
        || names.contains(&"seen.txt")
        || any_ext(".g00")
    {
        return Some("reallive");
    }
    if has(&|n| n.starts_with("siglusengine")) {
        return Some("siglus");
    }
    if any_ext(".xp3") {
        return Some("kirikiri");
    }
    if names.contains(&"nscript.dat") || any_ext(".nsa") {
        return Some("nscripter");
    }
    if names.contains(&"bgi.exe") || any_ext(".arc") {
        return Some("bgi");
    }
    if any_ext(".rpa") {
        return Some("renpy");
    }
    if names.contains(&"data.int") || any_ext(".cst") {
        return Some("catsystem2");
    }
    if any_ext(".pfs") || any_ext(".ald") {
        return Some("artemis");
    }
    None
}

/* ------------------------------------------------------------------ 打分 */

struct Evaluation {
    is_game: bool,
    score: i64,
}

fn evaluate(dir: &Path, facts: &DirFacts, tool_like: bool) -> Evaluation {
    let mut best: Vec<(&str, i64)> = Vec::new();
    let mut count_with_weight = 0usize;
    let mut data_bytes: u64 = 0;
    let mut max_exe_bytes: u64 = 0;
    let mut has_usable_exe = false;
    let mut has_launcher = false;
    let mut has_data_file = false;

    for (name, size) in &facts.files {
        let (ext, base) = split_ext(name);
        if ext.is_empty() {
            continue;
        }

        if ext == ".exe" {
            if !is_junk_exe(base) {
                has_usable_exe = true;
                max_exe_bytes = max_exe_bytes.max(*size);
            }
        } else if (ext == ".bat" || ext == ".cmd") && !is_junk_exe(base) {
            // 部分老日文游戏的主程序是「起動.bat」，没有 exe 也算"有可启动程序"
            has_launcher = true;
        } else if ext != ".dll" && ext != ".txt" && ext_weight(ext, base) >= 1 {
            data_bytes += size;
        }

        let weight = ext_weight(ext, base);
        if weight > 0 {
            count_with_weight += 1;
            if ext != ".exe" && ext != ".dll" && ext != ".txt" {
                has_data_file = true;
            }
            match best.iter_mut().find(|(e, _)| *e == ext) {
                Some(slot) => {
                    if weight > slot.1 {
                        slot.1 = weight;
                    }
                }
                None => best.push((ext, weight)),
            }
        }
    }

    let mut score: i64 = best.iter().map(|(_, w)| *w).sum();

    // 只有一个文件、而且是垃圾 exe → 分数封顶 2，别让它靠 4 分混过去
    if count_with_weight == 1
        && best.iter().any(|(e, _)| *e == ".exe")
        && facts
            .files
            .iter()
            .filter(|(n, _)| n.ends_with(".exe"))
            .all(|(n, _)| is_junk_exe(split_ext(n).1))
    {
        score = score.min(2);
    }

    let engine = engine_signature(facts);
    if engine.is_some() {
        score += 4;
    }

    let data_big = data_bytes >= BIG_DATA;
    if data_big {
        score += 2;
    }

    let has_data_dir = facts
        .subdir_names
        .iter()
        .any(|d| DATA_DIR_NAMES.contains(&d.as_str()));

    // 「数据在子目录」：Key 的 files\*.PAK、MUSICUS 的 base\*.dat 这类，
    // 顶层一个数据文件都没有，只看顶层会把它们整条漏掉。
    // 懒计算：前面已经有强信号就不必再翻子树了。
    let deep_data_big = !(has_data_file || data_big || engine.is_some())
        && deep_data_bytes(dir) >= BIG_DATA;

    let strong = has_data_file || data_big || deep_data_big || engine.is_some();
    let weak = max_exe_bytes >= BIG_EXE || has_data_dir;

    Evaluation {
        is_game: (has_usable_exe || has_launcher) && (strong || (weak && !tool_like)),
        score,
    }
}

/// 子树（深度 ≤2）里"数据类文件"的总字节数，到 5 MB 就提前返回
fn deep_data_bytes(dir: &Path) -> u64 {
    const MAX_ENTRIES: usize = 4000;
    let mut total: u64 = 0;
    let mut seen = 0usize;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];

    while let Some((cur, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&cur) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if seen >= MAX_ENTRIES || total >= BIG_DATA {
                return total;
            }
            seen += 1;
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                if depth < 2 {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let (ext, base) = split_ext(&name);
            if ext.is_empty() || ext == ".exe" || ext == ".dll" || ext == ".txt" {
                continue;
            }
            if ext_weight(ext, base) >= 1 {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

/* ------------------------------------------------------------ 容器（主程序在子目录） */

/// 这一层是不是"包着一部游戏"的容器目录。
///
/// 判据分两步。
///
/// **第一步：结构上像不像"游戏被拆在两个子目录里"**
/// 子树深度 ≤2 里要有数据文件（权重 ≥2），且满足其一：
/// - 只有恰好一个可启动程序；或
/// - 所有可启动程序都在**同一个子目录**；或
/// - 程序分布在不同子目录但**同名**，且目录名像"第几盘/第几部分"。
///
/// **第二步：程序和数据是不是被拆开了**（这一条是旧版没有的，必须补）
/// 旧规则只做第一步，于是 `会社\作品\game.exe + data.xp3` 这种"一个会社目录下只有一部游戏"
/// 的结构会被**误判成容器**，整个会社目录变成一条名为会社名的假游戏。
/// 真机上 `E:\galgame\yuzusoft\千恋万花` 正是这个形状。
///
/// 所以再加一条：只有当"程序所在目录"与"数据所在目录"**不是同一个**，
/// 或程序所在目录名本身就是 `bin` / `exe` 这类"只放程序"的目录时，才算容器。
/// 两者在同一个目录里 → 那个子目录自己就是一部完整的游戏，交给递归处理。
///
/// 为什么不能无条件合并：库根下两个游戏各在自己的子目录、主程序名不同 →
/// 三条都不满足 → 照旧下钻，避免把整库合并成一条。
fn is_container(dir: &Path) -> bool {
    const MAX_ENTRIES: usize = 5000;
    let mut stack = vec![(dir.to_path_buf(), String::new(), 0usize)];
    let mut seen = 0usize;
    // (base 名, 所在子目录相对路径)
    let mut exes: Vec<(String, String)> = Vec::new();
    // 数据文件（权重 ≥2）所在的子目录集合
    let mut data_dirs: HashSet<String> = HashSet::new();

    while let Some((cur, rel, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&cur) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if seen >= MAX_ENTRIES {
                break;
            }
            seen += 1;
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };

            if ft.is_dir() {
                if depth < 2 && !name.starts_with('.') && !is_skip_dir(&name) {
                    stack.push((entry.path(), child_rel, depth + 1));
                }
                continue;
            }

            let (ext, base) = split_ext(&name);
            if ext.is_empty() {
                continue;
            }
            if ext == ".exe" {
                if !is_junk_exe(base) {
                    exes.push((base.to_string(), rel.clone()));
                }
            } else if ext_weight(ext, base) >= 2 {
                data_dirs.insert(rel.clone());
            }
        }
    }

    if data_dirs.is_empty() || exes.is_empty() {
        return false;
    }

    // ---------- 第一步：结构判据 ----------
    let structurally_container = if exes.len() == 1 {
        true
    } else {
        let same_dir = exes.iter().all(|(_, d)| *d == exes[0].1);
        let same_base = exes.iter().all(|(b, _)| *b == exes[0].0);
        same_dir || (same_base && exes.iter().all(|(_, d)| is_part_dir(d)))
    };
    if !structurally_container {
        return false;
    }

    // ---------- 第二步：程序与数据是否被"拆在两个地方" ----------
    //
    // 真机上 `E:\galgame\nekonekosoft\作品\game.exe` + `作品\data\data.xp3` 这种
    // 「会社目录下只有一部游戏」的结构，旧规则会把会社目录整条收成游戏（假条目）。
    // 判据：只要程序和它的数据落在**同一个顶层子目录**里，那个子目录自己就是完整的游戏，
    // 这一层不是容器，应该继续往下钻。
    let exe_dirs: HashSet<String> = exes.iter().map(|(_, d)| d.clone()).collect();

    // 程序和数据在完全相同的一批目录里：只有 `bin/` 这种"只放程序"的目录才把父目录当容器
    if exe_dirs == data_dirs {
        return exe_dirs.iter().all(|d| is_program_dir(d));
    }

    let exe_tops: HashSet<&str> = exe_dirs.iter().map(|d| top_segment(d)).collect();
    let data_tops: HashSet<&str> = data_dirs.iter().map(|d| top_segment(d)).collect();
    if exe_tops.iter().any(|t| data_tops.contains(t)) {
        // 例：exe 在 `作品/`，数据在 `作品/data/` → 顶层段都是 `作品` → 不是容器
        return false;
    }

    // 例：exe 在 `bin/`，数据在 `data/` → 两个不同的顶层段 → 确实是"被拆开的游戏"
    true
}

/// 相对路径的顶层段：`bin/` → `bin`，`game/bin` → `game`，`` → ``
fn top_segment(rel: &str) -> &str {
    rel.split('/').next().unwrap_or(rel)
}

/// 目录名是不是"只放程序的目录"（`Game/bin/game.exe` 这种布局）
fn is_program_dir(dir: &str) -> bool {
    const PROGRAM_DIRS: &[&str] = &[
        "bin", "bin32", "bin64", "exe", "exes", "program", "programs", "binary", "binaries",
        "app", "startup", "起動", "启动",
    ];
    let last = dir.rsplit('/').next().unwrap_or(dir);
    PROGRAM_DIRS.contains(&last)
}

/* ------------------------------------------------------------ 启动候选排序 */

fn collect_exes(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_EXES {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_EXES {
            return;
        }
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();

        if ft.is_dir() {
            if lower.starts_with('.') || is_skip_dir(&lower) || is_patchy_dir(&lower) {
                continue;
            }
            collect_exes(&entry.path(), depth + 1, out);
        } else if ft.is_file() {
            let (ext, base) = split_ext(&lower);
            if (ext == ".exe" || ext == ".bat" || ext == ".cmd") && !is_junk_exe(base) {
                out.push(entry.path());
            }
        }
    }
}

/// 汉化/破解版优先 —— 真机上用户配的启动项几乎都是这个
fn is_preferred_exe(name: &str) -> bool {
    let lower = name.to_lowercase();
    if ["汉化", "中文", "破解", "整合"].iter().any(|k| lower.contains(k)) {
        return true;
    }
    // ASCII 关键词必须按"词边界"比，否则 `script.exe` 会因为含 `sc` 被误判
    for token in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
        if matches!(token, "chs" | "sc" | "cn" | "crack" | "chinese" | "cht" | "hardcoded") {
            return true;
        }
    }
    false
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// 归一化：只留字母数字和汉字，用于名字相似度比较
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 过滤 + 排序启动候选，最多 10 个
fn rank_exes(dir: &Path, folder_name: &str) -> Vec<(String, PathBuf)> {
    let mut raw = Vec::new();
    collect_exes(dir, 0, &mut raw);

    let query = normalize(folder_name);
    let mut scored: Vec<(usize, usize, usize, String, PathBuf)> = raw
        .into_iter()
        .map(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let preferred = if is_preferred_exe(&name) { 0 } else { 1 };
            let norm = normalize(&name);
            let dist = if query.is_empty() {
                0
            } else if norm.contains(&query) {
                0
            } else {
                levenshtein(&norm, &query)
            };
            (preferred, dist, name.len(), name, p)
        })
        .collect();

    scored.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    scored
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, _, _, name, path)| (name, path))
        .collect()
}

/* -------------------------------------------------------------- 统计与组装 */

/// 递归统计（文件数, 总字节数）。到上限就停，避免指到盘符根时卡死。
fn dir_stats(dir: &Path) -> (i64, u64) {
    let mut count: i64 = 0;
    let mut bytes: u64 = 0;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(cur) = stack.pop() {
        let entries = match std::fs::read_dir(&cur) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if count as usize >= STATS_MAX_FILES {
                return (count, bytes);
            }
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                count += 1;
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    (count, bytes)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExeCandidate {
    pub name: String,
    pub path: String,
}

/// 扫描出的一条游戏，字段与 `games` 表一一对应（除了那些扫描不该碰的元数据列）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedGame {
    pub path_hash: String,
    pub folder_path: String,
    pub folder_name: String,
    pub engine: String,
    pub launch_exe: String,
    pub launch_candidates: Vec<ExeCandidate>,
    pub file_count: i64,
    pub dir_size: u64,
    pub match_score: i64,
}

fn make_game(dir: &Path, score: i64) -> ScannedGame {
    let folder_path = dir.to_string_lossy().to_string();
    let folder_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| folder_path.clone());

    let candidates = rank_exes(dir, &folder_name);
    let best_exe = candidates.first().map(|(_, p)| p.clone());

    // 引擎：先看目录指纹（更准，会扫两层），认不出来再读主程序特征串
    let engine = engine_probe::detect(dir)
        .map(|e| e.to_string())
        .or_else(|| {
            best_exe
                .as_deref()
                .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false))
                .and_then(engine_probe::detect_from_exe)
                .map(|e| e.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    let (file_count, dir_size) = dir_stats(dir);

    ScannedGame {
        path_hash: path_hash(&folder_path),
        folder_path,
        folder_name,
        engine,
        launch_exe: best_exe
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        launch_candidates: candidates
            .into_iter()
            .map(|(name, path)| ExeCandidate {
                name,
                path: path.to_string_lossy().to_string(),
            })
            .collect(),
        file_count,
        dir_size,
        match_score: score,
    }
}

/* ------------------------------------------------------------------ 进度 */

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// 已处理目录数
    pub dirs_scanned: usize,
    /// 已识别游戏数
    pub games_found: usize,
    /// 当前正在看的目录
    pub current: String,
}

/* ------------------------------------------------------------------ 入口 */

/// 扫描入口。`on_progress` 会被**节流**调用（约 10 次/秒），并在每发现一部游戏时立刻调一次。
///
/// 之所以不"每扫到一个目录就发一次"：一本 200 部的库轻松有上万个目录，
/// 上万个 IPC 事件会把 webview 的消息队列打满，进度条反而卡住。
/// 节流后前端看到的是"每 100ms 一帧"，视觉上更顺。
pub fn scan_root<F>(root: &Path, mut on_progress: F) -> Result<Vec<ScannedGame>, String>
where
    F: FnMut(&ScanProgress),
{
    if !root.is_dir() {
        return Err(format!("目录不存在或无法读取：{}", root.display()));
    }

    let mut games: Vec<ScannedGame> = Vec::new();
    let mut progress = ScanProgress::default();
    let mut last_emit = Instant::now() - PROGRESS_INTERVAL;
    // 同一个文件夹只记一次（junction / 重复挂载可能导致同一路径被走到两次）
    let mut seen_paths: HashSet<String> = HashSet::new();

    visit(
        root,
        0,
        &mut games,
        &mut progress,
        &mut seen_paths,
        &mut on_progress,
        &mut last_emit,
    );

    // 收尾再发一次，前端据此知道结束了
    progress.current.clear();
    on_progress(&progress);
    Ok(games)
}

fn visit<F>(
    dir: &Path,
    depth: usize,
    games: &mut Vec<ScannedGame>,
    progress: &mut ScanProgress,
    seen: &mut HashSet<String>,
    on_progress: &mut F,
    last_emit: &mut Instant,
) where
    F: FnMut(&ScanProgress),
{
    if depth > MAX_DEPTH {
        return;
    }

    progress.dirs_scanned += 1;
    progress.current = dir.to_string_lossy().to_string();
    if last_emit.elapsed() >= PROGRESS_INTERVAL {
        on_progress(progress);
        *last_emit = Instant::now();
    }

    let facts = match read_facts(dir) {
        Some(f) => f,
        None => return, // 权限不足 / 目录没了：跳过，不影响其它游戏
    };

    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tool_like = is_tool_like_name(&name);
    let ev = evaluate(dir, &facts, tool_like);

    if ev.is_game {
        let game = make_game(dir, ev.score);
        if seen.insert(game.folder_path.clone()) {
            games.push(game);
            progress.games_found += 1;
            // 发现游戏立刻发一次，前端能马上看到计数在涨
            on_progress(progress);
            *last_emit = Instant::now();
        }
        // 已经认定这一层是游戏，不再下钻 —— 否则子目录会被拆成独立条目
        return;
    }

    if is_container(dir) {
        let game = make_game(dir, ev.score);
        if seen.insert(game.folder_path.clone()) {
            games.push(game);
            progress.games_found += 1;
            on_progress(progress);
            *last_emit = Instant::now();
        }
        return;
    }

    for sub in &facts.subdirs {
        let lower = sub.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if is_skip_dir(&lower) {
            continue;
        }
        visit(sub, depth + 1, games, progress, seen, on_progress, last_emit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("moeshelf-scan-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 建一个文件（自动补齐父目录），返回路径
    fn touch(root: &Path, rel: &str) -> PathBuf {
        let p = root.join(rel.replace('/', &std::path::MAIN_SEPARATOR.to_string()));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, b"x").unwrap();
        p
    }

    fn names(games: &[ScannedGame]) -> Vec<String> {
        let mut v: Vec<String> = games.iter().map(|g| g.folder_name.clone()).collect();
        v.sort();
        v
    }

    /* ---------------------------------------------------------- 哈希 */

    /// 这三个期望值是用旧版同款 djb2（按 UTF-16 码元）算出来的。
    /// 改这个函数 = 让已迁移的 211 部全部对不上，务必别动。
    #[test]
    fn path_hash_matches_legacy() {
        assert_eq!(path_hash("abc"), "0b885c8b");
        assert_eq!(path_hash("千恋万花"), "aba9708b");
        assert_eq!(
            path_hash("E:\\galgame\\yuzusoft\\千恋万花"),
            "9c874865",
            "这个值与旧库里 千恋万花 的 path_hash 一致"
        );
    }

    /* ---------------------------------------------------------- 基本判定 */

    #[test]
    fn detects_simple_game() {
        let root = temp_root("simple");
        touch(&root, "game.exe");
        touch(&root, "data.xp3");

        let games = scan_root(&root, |_| {}).unwrap();
        assert_eq!(names(&games), vec!["moeshelf-scan-simple"]);
        assert_eq!(games[0].engine, "kirikiri");
        assert!(games[0].launch_exe.ends_with("game.exe"));
        assert_eq!(games[0].match_score, 14, "xp3(6) + exe(4) + 引擎特征(4)");
    }

    #[test]
    fn rejects_tool_directory() {
        // 只有安装器 + 说明：不能算游戏
        let root = temp_root("tool");
        touch(&root, "setup.exe");
        touch(&root, "readme.txt");
        assert!(scan_root(&root, |_| {}).unwrap().is_empty());
    }

    #[test]
    fn requires_solid_evidence() {
        // 有非垃圾 exe，但没有任何数据文件、exe 也不大、没有 data 目录 →
        // 这是「一堆小工具的目录」，不是游戏
        let root = temp_root("noevidence");
        touch(&root, "foo.exe");
        touch(&root, "helper.dll");
        touch(&root, "说明.txt");
        assert!(scan_root(&root, |_| {}).unwrap().is_empty());
    }

    #[test]
    fn bat_launcher_counts_as_executable() {
        // 部分老日文游戏主程序是 起動.bat，没有 exe
        let root = temp_root("bat");
        touch(&root, "起動.bat");
        touch(&root, "data.xp3");
        assert_eq!(scan_root(&root, |_| {}).unwrap().len(), 1);
    }

    /* ---------------------------------------------------------- 容器 */

    #[test]
    fn exe_in_subdir_is_one_game_not_two() {
        // 主程序在 bin/、数据在 data/ → 必须认成**一个**游戏（父目录），
        // 而不是拆成 bin 和 data 两条
        let root = temp_root("container");
        touch(&root, "bin/game.exe");
        touch(&root, "data/data.xp3");

        let games = scan_root(&root, |_| {}).unwrap();
        assert_eq!(names(&games), vec!["moeshelf-scan-container"]);
        assert!(games[0].launch_exe.contains("bin"), "启动项应指向子目录里的 exe");
    }

    #[test]
    fn library_with_two_games_is_not_merged() {
        let root = temp_root("library");
        touch(&root, "A/game.exe");
        touch(&root, "A/data.xp3");
        touch(&root, "B/other.exe");
        touch(&root, "B/data.pfs");

        let games = scan_root(&root, |_| {}).unwrap();
        assert_eq!(names(&games), vec!["A", "B"]);
        assert_eq!(games.len(), 2);
    }

    #[test]
    fn nested_library_depth_is_respected() {
        let root = temp_root("nested");
        touch(&root, "会社1/作品1/game.exe");
        touch(&root, "会社1/作品1/data.xp3");
        touch(&root, "会社2/作品2/game.exe");
        touch(&root, "会社2/作品2/data.pfs");

        let games = scan_root(&root, |_| {}).unwrap();
        assert_eq!(names(&games), vec!["作品1", "作品2"]);
    }

    /* ---------------------------------------------------------- 启动候选 */

    #[test]
    fn patch_dir_exe_is_not_a_launch_candidate() {
        let root = temp_root("patchdir");
        touch(&root, "game.exe");
        touch(&root, "data.xp3");
        touch(&root, "补丁/汉化启动.exe");

        let games = scan_root(&root, |_| {}).unwrap();
        let cands = &games[0].launch_candidates;
        assert_eq!(cands.len(), 1, "补丁目录里的 exe 不该进候选：{cands:?}");
        assert_eq!(cands[0].name, "game.exe");
    }

    #[test]
    fn chinese_patched_exe_ranks_first() {
        let root = temp_root("prefer");
        touch(&root, "game.exe");
        touch(&root, "game_chs.exe");
        touch(&root, "data.xp3");

        let games = scan_root(&root, |_| {}).unwrap();
        assert_eq!(games[0].launch_candidates[0].name, "game_chs.exe");
    }

    /* ---------------------------------------------------------- 统计 */

    #[test]
    fn stats_are_recursive() {
        let root = temp_root("stats");
        touch(&root, "game.exe");
        touch(&root, "data.xp3");
        touch(&root, "save/a.bin");
        touch(&root, "save/b.bin");

        let games = scan_root(&root, |_| {}).unwrap();
        // 4 个文件（game.exe / data.xp3 / save/a.bin / save/b.bin）
        assert_eq!(games[0].file_count, 4);
        assert_eq!(games[0].dir_size, 4, "每个文件写了 1 字节");
    }

    #[test]
    fn progress_callback_reports_counts() {
        let root = temp_root("progress");
        touch(&root, "A/game.exe");
        touch(&root, "A/data.xp3");
        touch(&root, "B/game.exe");
        touch(&root, "B/data.xp3");

        let mut last = ScanProgress::default();
        let games = scan_root(&root, |p| {
            assert!(
                p.dirs_scanned >= last.dirs_scanned,
                "已扫描目录数不能倒退"
            );
            last = p.clone();
        })
        .unwrap();

        assert_eq!(games.len(), 2);
        assert_eq!(last.games_found, 2, "最后一次回调应报告 2 部");
        assert!(last.dirs_scanned > 0);
    }

    #[test]
    fn missing_root_is_an_error() {
        assert!(scan_root(Path::new("Z:\\definitely\\not\\here"), |_| {}).is_err());
    }
}

/// 真机验收：对着真实的 `E:/galgame` 跑一遍，**只扫描不写库**，
/// 然后把结果和 dev 库里已迁移的 211 部做对比。
///
/// 手动跑：
/// `cargo test scan_real_library -- --ignored --nocapture`
#[cfg(test)]
mod real_library {
    use super::*;
    use rusqlite::{Connection, OpenFlags};
    use std::collections::HashMap;

    #[test]
    #[ignore]
    fn scan_real_library() {
        // 注意：`PathBuf::from("E:")` 是「盘符相对路径」，拼出来是 `E:galgame`（错的）。
        // 必须带上根分隔符，才是盘符根目录 `E:\`。
        let root = std::path::PathBuf::from(format!("E:{}", std::path::MAIN_SEPARATOR)).join("galgame");
        if !root.is_dir() {
            println!("跳过：本机没有 {} 目录", root.display());
            return;
        }

        println!("开始扫描：{}", root.display());
        let started = Instant::now();
        let mut tick = Instant::now();
        let mut dirs = 0usize;
        let games = scan_root(&root, |p| {
            dirs = p.dirs_scanned;
            if tick.elapsed().as_millis() >= 2000 {
                println!(
                    "  ... 已扫 {} 个目录，找到 {} 部",
                    p.dirs_scanned, p.games_found
                );
                tick = Instant::now();
            }
        })
        .expect("扫描失败");

        println!(
            "\n扫描完成：{} 部 · 扫过 {} 个目录 · 耗时 {:.1}s",
            games.len(),
            dirs,
            started.elapsed().as_secs_f64()
        );

        // 引擎分布
        let mut engines: HashMap<&str, usize> = HashMap::new();
        for g in &games {
            *engines.entry(g.engine.as_str()).or_insert(0) += 1;
        }
        let mut dist: Vec<(&str, usize)> = engines.into_iter().collect();
        dist.sort_by(|a, b| b.1.cmp(&a.1));
        println!("\n引擎分布：");
        for (e, n) in dist {
            println!("  {e:<12} {n}");
        }

        // 和已迁移的库比对
        let db_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("moeshelf.db");
        if !db_path.exists() {
            println!("\n(dev 库不存在，跳过比对)");
            return;
        }
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("只读打开 dev 库失败");
        let mut stmt = conn.prepare("SELECT path_hash, launch_exe FROM games").unwrap();
        let existing: HashMap<String, String> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .flatten()
            .collect();

        let hit: Vec<&ScannedGame> = games
            .iter()
            .filter(|g| existing.contains_key(&g.path_hash))
            .collect();
        let new: Vec<&ScannedGame> = games
            .iter()
            .filter(|g| !existing.contains_key(&g.path_hash))
            .collect();

        println!("\n与 dev 库比对（库内 {} 条）：", existing.len());
        println!("  扫描结果能对上已有条目：{}", hit.len());
        println!("  扫描出的新条目：{}", new.len());

        // 库里有、但这次扫描没找到的（可能已删除 / 已移动 / 被规则误杀）
        let scanned_hashes: HashSet<&str> = games.iter().map(|g| g.path_hash.as_str()).collect();
        let missing: Vec<&String> = existing
            .keys()
            .filter(|h| !scanned_hashes.contains(h.as_str()))
            .collect();
        println!("  库里有但没扫到的：{}", missing.len());
        if !missing.is_empty() {
            for h in missing.iter().take(15) {
                println!("    未扫到 hash={h}");
            }
        }

        // 对上已有条目时，主程序和引擎是否更准
        let mut exe_diff = 0;
        for g in &hit {
            let old = existing.get(&g.path_hash).cloned().unwrap_or_default();
            if !old.is_empty() && !g.launch_exe.is_empty() && old != g.launch_exe {
                exe_diff += 1;
            }
        }
        println!("  其中主程序判定与旧库不同的：{exe_diff}");

        if !new.is_empty() {
            println!("\n新条目样例（前 10）：");
            for g in new.iter().take(10) {
                println!(
                    "  {}  [{}, {} 个文件]  {}",
                    g.folder_name,
                    g.engine,
                    g.file_count,
                    g.launch_exe.rsplit(std::path::MAIN_SEPARATOR).next().unwrap_or("-")
                );
            }
        }

        println!("\n扫描结果抽样（前 10）：");
        for g in games.iter().take(10) {
            println!(
                "  {:<28} {:<10} 主程序={:<24} 分数={}",
                g.folder_name,
                g.engine,
                g.launch_exe.rsplit(std::path::MAIN_SEPARATOR).next().unwrap_or("-"),
                g.match_score
            );
        }
    }
}

#[cfg(test)]
mod skip_tests {
    use super::*;

    fn root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("moeshelf-skip-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(base: &Path, rel: &str) {
        let p = base.join(rel.replace('/', &std::path::MAIN_SEPARATOR.to_string()));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, b"x").unwrap();
    }

    /// 这两个目录名来自真机扫描结果：它们都被当成过游戏，必须硬跳过
    #[test]
    fn temp_and_python_dirs_are_skipped() {
        let base = root("tools");
        // 一部真游戏
        touch(&base, "作品/game.exe");
        touch(&base, "作品/data.xp3");
        // 两个假游戏
        touch(&base, "临时文件/启动.bat");
        touch(&base, "临时文件/data.bin");
        touch(&base, "python/python.exe");
        touch(&base, "python/data.dat");

        let games = scan_root(&base, |_| {}).unwrap();
        let names: Vec<&str> = games.iter().map(|g| g.folder_name.as_str()).collect();
        assert_eq!(names, vec!["作品"], "临时文件 / python 不该被当成游戏");
    }
}

/// 端到端：真实扫描 → 真实写库，但写在**库的副本**上，不动正式库。
///
/// 手动跑：`cargo test scan_into_temp_db -- --ignored --nocapture`
#[cfg(test)]
mod end_to_end {
    use super::*;
    use crate::db::{open_at, repo};

    #[test]
    #[ignore]
    fn scan_into_temp_db() {
        let root = std::path::PathBuf::from(format!("E:{}", std::path::MAIN_SEPARATOR)).join("galgame");
        if !root.is_dir() {
            println!("跳过：本机没有 {}", root.display());
            return;
        }

        // 1. 复制一份 dev 库到临时目录
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("moeshelf.db");
        let dir = std::env::temp_dir().join("moeshelf-e2e");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dst = dir.join("moeshelf.db");
        if src.exists() {
            for suffix in ["", "-wal", "-shm"] {
                let from = src.with_file_name(format!("moeshelf.db{suffix}"));
                if from.exists() {
                    std::fs::copy(&from, dst.with_file_name(format!("moeshelf.db{suffix}")))
                        .unwrap();
                }
            }
        }
        println!("工作库（副本）：{}", dst.display());

        let conn = open_at(&dst).expect("打开副本失败");
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
            .unwrap();
        println!("扫描前：{before} 条");

        // 2. 真扫描
        let started = Instant::now();
        let games = scan_root(&root, |_| {}).expect("扫描失败");
        println!("扫描到 {} 部，耗时 {:.1}s", games.len(), started.elapsed().as_secs_f64());

        // 3. 真写库（一个事务）
        let tx = conn.unchecked_transaction().unwrap();
        let mut inserted = 0;
        let mut updated = 0;
        for g in &games {
            if repo::upsert_scanned(&tx, g).expect("写入失败") {
                inserted += 1;
            } else {
                updated += 1;
            }
        }
        tx.commit().unwrap();

        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
            .unwrap();
        println!("写库结果：新增 {inserted} · 更新 {updated}");
        println!("扫描后：{after} 条（应为 {} + {inserted}）", before);

        assert_eq!(after, before + inserted as i64, "行数必须对得上");

        // 4. 抽查：新行有目录大小、老行标题没被冲掉
        let with_size: i64 = conn
            .query_row("SELECT COUNT(*) FROM games WHERE dir_size > 0", [], |r| r.get(0))
            .unwrap();
        println!("有目录大小的行：{with_size} / {after}");
        assert!(with_size >= games.len() as i64 - 1, "扫描出来的行都该有 dir_size");

        let sample: (String, i64, String) = conn
            .query_row(
                "SELECT title_cn, dir_size, engine FROM games WHERE dir_size > 0 LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        println!("抽样：{} · {} 字节 · 引擎 {}", sample.0, sample.1, sample.2);
    }
}
