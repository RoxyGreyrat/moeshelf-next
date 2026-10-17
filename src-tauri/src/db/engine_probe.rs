//! 引擎指纹探测：只看文件名，不读文件内容。
//!
//! 背景：旧库的 `matched_types` 只记录扫描器白名单里的少数扩展名，
//! 211 部里有 111 部没有 `engine:` 标记。但这些游戏的目录大多还在硬盘上，
//! 只要看一眼目录里的特征文件（`data.xp3` / `UnityPlayer.dll` / `.pfs` …）就能认出来。
//!
//! 原则：**宁可返回 None 也不猜**。识别不出来就留 unknown，交给 D 阶段的刮削或人工指定。

use std::collections::HashSet;
use std::path::Path;

/// 探测深度：根目录 + 一层子目录。Galgame 的特征文件不会埋得更深。
const MAX_DEPTH: usize = 2;
/// 单目录最多看多少个条目，避免遇到巨大目录时拖慢迁移
const MAX_ENTRIES: usize = 4000;

struct Scan {
    files: HashSet<String>,
    dirs: HashSet<String>,
}

impl Scan {
    fn has_file(&self, name: &str) -> bool {
        self.files.contains(name)
    }
    fn has_ext(&self, ext: &str) -> bool {
        self.files.iter().any(|f| f.ends_with(ext))
    }
    fn has_dir_suffix(&self, suffix: &str) -> bool {
        self.dirs.iter().any(|d| d.ends_with(suffix))
    }
}

fn scan(dir: &Path) -> Option<Scan> {
    let mut files = HashSet::new();
    let mut dirs = HashSet::new();
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    let mut seen = 0usize;

    while let Some((path, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&path) {
            Ok(e) => e,
            // 权限不足 / 目录不存在：跳过这一层，不影响其它游戏
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            if seen >= MAX_ENTRIES {
                break;
            }
            seen += 1;

            let name = entry.file_name().to_string_lossy().to_lowercase();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

            if is_dir {
                dirs.insert(name);
                if depth + 1 < MAX_DEPTH {
                    stack.push((entry.path(), depth + 1));
                }
            } else {
                files.insert(name);
            }
        }
    }

    if files.is_empty() && dirs.is_empty() {
        None
    } else {
        Some(Scan { files, dirs })
    }
}

/// 返回引擎标识；认不出来返回 None。
///
/// 判定顺序 = 从"最强特征"到"最弱特征"，靠前的命中就返回：
/// `.xp3` 只有 KiriKiri 用；`UnityPlayer.dll` 只有 Unity 有；`.pck` 是 Siglus；
/// `.pfs` 是 Artemis；`.arc` 是 BGI（但部分老引擎也用 .arc，所以排在后面）。
pub fn detect(dir: &Path) -> Option<&'static str> {
    if !dir.is_dir() {
        return None;
    }
    let s = scan(dir)?;

    // 0. 文件名里直接写着引擎名的（SiglusEngine.exe / Ethornell.dll / Majiro.exe …）
    if let Some(found) = name_hint(&s) {
        return Some(found);
    }

    // 1. KiriKiri：data.xp3 / *.xp3 是该引擎独有的封装
    if s.has_ext(".xp3") {
        return Some("kirikiri");
    }

    // 2. Unity：UnityPlayer.dll 或 xxx_Data 目录
    if s.has_file("unityplayer.dll") || s.has_dir_suffix("_data") {
        return Some("unity");
    }

    // 3. Siglus：.pck（SiglusEngine.pck / Scene.pck）
    if s.has_ext(".pck") || s.has_file("siglusengine.exe") {
        return Some("siglus");
    }

    // 4. Artemis：.pfs
    if s.has_ext(".pfs") {
        return Some("artemis");
    }

    // 5. Ren'Py：.rpa 或 renpy/ 目录
    if s.has_ext(".rpa") || s.dirs.contains("renpy") {
        return Some("renpy");
    }

    // 6. BGI：.arc（Ethornell）
    if s.has_ext(".arc") {
        return Some("bgi");
    }

    // 7. YU-RIS：.ypf
    if s.has_ext(".ypf") {
        return Some("yuris");
    }

    // 8. CatSystem2：.int + .dat 同时出现才认（单独 .int 太泛）
    if s.has_ext(".int") && s.has_ext(".dat") {
        return Some("catsystem2");
    }

    None
}

/// 文件名/目录名里直接出现的引擎关键词。命中率很高且几乎不会误判
/// （文件被命名成 `SiglusEngine.exe` 或 `Ethornell.dll` 时，基本就是它）。
fn name_hint(s: &Scan) -> Option<&'static str> {
    const HINTS: &[(&str, &str)] = &[
        ("siglus", "siglus"),
        ("ethornell", "bgi"),
        ("majiro", "majiro"),
        ("nscripter", "nscripter"),
        ("advhd", "advhd"),
        ("agf", "advhd"), // AdvHD 的图形资源后缀
        ("kirikiri", "kirikiri"),
        ("catsystem", "catsystem2"),
        ("system40", "system4"),
        ("reallive", "reallive"),
        ("artemis", "artemis"),
        ("yuris", "yuris"),
        ("unityplayer", "unity"),
        ("renpy", "renpy"),
    ];

    for name in s.files.iter().chain(s.dirs.iter()) {
        for (needle, engine) in HINTS {
            if name.contains(needle) {
                return Some(engine);
            }
        }
    }
    None
}

/// 读主程序内容找引擎标识串。
///
/// 只读前 4 MB（引擎标识通常在 PE 头/导入表/资源区，靠前）。
/// 同时搜两种编码：普通 ASCII，以及去掉所有 0x00 后的版本 —— 后者能把
/// UTF-16LE 的 `S\0i\0g\0l\0u\0s\0` 压成 `Siglus`，一并命中。
pub fn detect_from_exe(exe: &Path) -> Option<&'static str> {
    const MAX: usize = 4 * 1024 * 1024;

    let data = {
        use std::io::Read;
        let mut f = std::fs::File::open(exe).ok()?;
        let mut buf = Vec::with_capacity(MAX.min(1 << 20));
        f.by_ref().take(MAX as u64).read_to_end(&mut buf).ok()?;
        buf
    };
    if data.is_empty() {
        return None;
    }

    // 转小写，便于不区分大小写搜索
    let lower: Vec<u8> = data
        .iter()
        .map(|b| if b.is_ascii_uppercase() { b + 32 } else { *b })
        .collect();
    // 去掉 0x00，把 UTF-16LE 的 ASCII 串压成普通串
    let squashed: Vec<u8> = lower.iter().copied().filter(|b| *b != 0).collect();

    const MARKERS: &[(&str, &str)] = &[
        ("ethornell", "bgi"),
        ("siglusengine", "siglus"),
        ("siglus", "siglus"),
        ("kirikiri", "kirikiri"),
        ("tvp/", "kirikiri"), // KiriKiri 内核 TVP 的路径常量
        ("majiro", "majiro"),
        ("nscripter", "nscripter"),
        ("advhd", "advhd"),
        ("catsystem", "catsystem2"),
        ("system4.0", "system4"),
        ("system40", "system4"),
        ("reallive", "reallive"),
        ("yu-ris", "yuris"),
        ("yuris", "yuris"),
        ("artemis engine", "artemis"),
        ("unityplayer", "unity"),
        ("ren'py", "renpy"),
    ];

    for (needle, engine) in MARKERS {
        let pat = needle.as_bytes();
        if contains(&lower, pat) || contains(&squashed, pat) {
            return Some(engine);
        }
    }
    None
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个临时目录，写入给定文件名，跑完自动清理
    fn fixture(tag: &str, files: &[&str], dirs: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("moeshelf-probe-{tag}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for f in files {
            let p = root.join(f);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, b"x").unwrap();
        }
        for d in dirs {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        root
    }

    #[test]
    fn detects_kirikiri() {
        let d = fixture("kk", &["data.xp3", "game.exe"], &[]);
        assert_eq!(detect(&d), Some("kirikiri"));
    }

    #[test]
    fn detects_unity_by_dll_or_data_dir() {
        let a = fixture("u1", &["UnityPlayer.dll", "game.exe"], &[]);
        assert_eq!(detect(&a), Some("unity"));
        let b = fixture("u2", &["game.exe"], &["Game_Data"]);
        assert_eq!(detect(&b), Some("unity"));
    }

    #[test]
    fn detects_siglus_and_artemis() {
        assert_eq!(detect(&fixture("sg", &["Scene.pck"], &[])), Some("siglus"));
        assert_eq!(detect(&fixture("ar", &["data.pfs"], &[])), Some("artemis"));
    }

    #[test]
    fn detects_renpy_bgi_yuris() {
        assert_eq!(detect(&fixture("rp", &["archive.rpa"], &[])), Some("renpy"));
        assert_eq!(detect(&fixture("bg", &["data.arc"], &[])), Some("bgi"));
        assert_eq!(detect(&fixture("yr", &["ysbin.ypf"], &[])), Some("yuris"));
    }

    #[test]
    fn subdirectory_files_are_seen() {
        // 特征文件在下一层子目录里也要能认出来
        let d = fixture("deep", &["media/data.xp3"], &[]);
        assert_eq!(detect(&d), Some("kirikiri"));
    }

    #[test]
    fn refuses_to_guess() {
        // 只有 exe/dll/txt —— 这种情况必须返回 None，不能瞎猜
        let d = fixture("none", &["game.exe", "engine.dll", "readme.txt"], &[]);
        assert_eq!(detect(&d), None);
    }

    #[test]
    fn exe_content_ascii_and_utf16() {
        let dir = std::env::temp_dir().join("moeshelf-probe-exe");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 普通 ASCII
        let a = dir.join("a.exe");
        std::fs::write(&a, b"MZ....Ethornell....").unwrap();
        assert_eq!(detect_from_exe(&a), Some("bgi"));

        // UTF-16LE：S\0i\0g\0l\0u\0s\0
        let b = dir.join("b.exe");
        let mut bytes: Vec<u8> = b"MZ".to_vec();
        for c in "SiglusEngine".bytes() {
            bytes.push(c);
            bytes.push(0);
        }
        std::fs::write(&b, &bytes).unwrap();
        assert_eq!(detect_from_exe(&b), Some("siglus"));

        // 没有特征串 → None
        let c = dir.join("c.exe");
        std::fs::write(&c, b"MZ....nothing here....").unwrap();
        assert_eq!(detect_from_exe(&c), None);
    }

    #[test]
    fn name_hint_beats_generic_extensions() {
        // 只有 .arc（通常判 bgi），但文件名写了 SiglusEngine → 以文件名为准
        let d = fixture("hint", &["SiglusEngine.exe", "data.arc"], &[]);
        assert_eq!(detect(&d), Some("siglus"));
    }

    #[test]
    fn missing_dir_is_none() {
        assert_eq!(detect(Path::new("Z:\\definitely\\not\\here")), None);
    }
}
