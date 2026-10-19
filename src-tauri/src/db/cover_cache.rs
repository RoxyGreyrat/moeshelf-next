//! 旧库封面缓存（方案 A：直读旧目录，零拷贝）。
//!
//! 位置：`E:\moeshelf\moeshelf-build-2.1.0\data\cache\images\`
//! 结构：每张图两个文件 —— `<8位hex>.bin`（原始字节，无扩展名）+ `<8位hex>.json`（记 contentType）。
//! 文件名 = `djb2(封面URL)`，与旧项目 PathHash 逐位一致（实测 211/211 命中）。
//!
//! 为什么不能直接用 convertFileSrc：
//!   文件没有扩展名，WebView 会按 application/octet-stream 返回，`<img>` 不一定会解码。
//!   这里注册自定义协议 `cover://<hash>`，由 Rust 读文件并显式带上 Content-Type。

use std::path::{Path, PathBuf};

/// 可用环境变量覆盖，便于以后把缓存挪走
pub fn dir() -> PathBuf {
    if let Ok(custom) = std::env::var("MOESHELF_COVER_DIR") {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom);
        }
    }
    PathBuf::from(r"E:\moeshelf\moeshelf-build-2.1.0\data\cache\images")
}

/// 只接受 8 位十六进制，挡住 `../` 之类的路径穿越
pub fn is_valid_key(key: &str) -> bool {
    key.len() == 8 && key.chars().all(|c| c.is_ascii_hexdigit())
}

/// 读旁挂 json 里的 contentType，读不到就按 JPEG 兜底
pub fn content_type(base: &Path, key: &str) -> String {
    let sidecar = base.join(format!("{key}.json"));
    if let Ok(raw) = std::fs::read_to_string(sidecar) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(ct) = v.get("contentType").and_then(|x| x.as_str()) {
                if !ct.trim().is_empty() {
                    return ct.to_string();
                }
            }
        }
    }
    "image/jpeg".to_string()
}

/* ------------------------------ 用户自定义封面 ------------------------------ */

/// 用户自己选的封面（本地图片或下载下来的 URL）存在应用的 `data/covers/`。
///
/// 文件名同样是 `djb2(来源字符串)` —— 来源是本地路径时就是 `djb2(路径)`，
/// 是 URL 时就是 `djb2(url)`。这样前端 `coverSrc()` 一行都不用改：
/// 它只负责把封面来源哈希成 8 位 hex，协议层决定去哪个目录取。
pub fn local_dir(app: &tauri::AppHandle) -> PathBuf {
    match super::data_dir(app) {
        Ok(dir) => dir.join("covers"),
        Err(_) => PathBuf::from("covers"),
    }
}

/// 按文件头判断图片类型。用户选的图可能是 png/webp，不能一律当 jpeg 返回。
pub fn sniff_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 2 && bytes[0] == b'B' && bytes[1] == b'M' {
        "image/bmp"
    } else {
        "application/octet-stream"
    }
}

/// 是不是一个能当封面用的图片
pub fn is_image_bytes(bytes: &[u8]) -> bool {
    sniff_content_type(bytes).starts_with("image/")
}

/// 写进应用自建封面缓存，返回写入的文件路径
pub fn store(app: &tauri::AppHandle, key: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    if !is_valid_key(key) {
        return Err(format!("非法的封面键：{key}"));
    }
    let dir = local_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建封面目录失败：{e}"))?;
    let path = dir.join(format!("{key}.bin"));
    std::fs::write(&path, bytes).map_err(|e| format!("写入封面失败：{e}"))?;
    Ok(path)
}

/// 先查应用自建缓存（用户自己换过的封面优先），再回落到旧库缓存。
pub fn read_any(app: &tauri::AppHandle, key: &str) -> Option<(Vec<u8>, String)> {
    if !is_valid_key(key) {
        return None;
    }

    let local = local_dir(app).join(format!("{key}.bin"));
    if let Ok(bytes) = std::fs::read(&local) {
        if !bytes.is_empty() {
            let ct = sniff_content_type(&bytes).to_string();
            return Some((bytes, ct));
        }
    }

    let base = dir();
    read(&base, key).map(|b| (b, content_type(&base, key)))
}

/// 读取封面字节。key 非法或文件不存在都返回 None。
pub fn read(base: &Path, key: &str) -> Option<Vec<u8>> {
    if !is_valid_key(key) {
        return None;
    }
    std::fs::read(base.join(format!("{key}.bin"))).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_image_types() {
        assert_eq!(sniff_content_type(&[0xFF, 0xD8, 0x00]), "image/jpeg");
        assert_eq!(sniff_content_type(&[0x89, 0x50, 0x4E, 0x47, 0x00]), "image/png");
        assert_eq!(sniff_content_type(b"GIF89a"), "image/gif");
        assert_eq!(sniff_content_type(b"not an image"), "application/octet-stream");
        assert!(is_image_bytes(&[0xFF, 0xD8, 0x00]));
        assert!(!is_image_bytes(b"nope"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_valid_key("../etc"));
        assert!(!is_valid_key("8b0d016"));
        assert!(!is_valid_key("8b0d01655"));
        assert!(!is_valid_key("zzzzzzzz"));
        assert!(is_valid_key("8b0d0165"));
        assert!(is_valid_key("A1B2C3D4"));
    }

    #[test]
    fn reads_existing_cover_and_content_type() {
        let base = dir();
        // 取一个真实存在的封面做冒烟测试；缓存目录不在就跳过
        let Ok(entries) = std::fs::read_dir(&base) else {
            println!("跳过：本机没有封面缓存目录");
            return;
        };
        let sample = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .find(|n| n.ends_with(".bin"));

        let Some(name) = sample else {
            println!("跳过：缓存目录为空");
            return;
        };
        let key = name.trim_end_matches(".bin");

        let bytes = read(&base, key).expect("应能读到封面字节");
        assert!(!bytes.is_empty());
        // JPEG / PNG / WebP 的魔数特征
        let is_image = bytes.starts_with(&[0xFF, 0xD8])
            || bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47])
            || (bytes.len() > 12 && &bytes[8..12] == b"WEBP");
        assert!(is_image, "封面应是图片格式，实际前 4 字节：{:?}", &bytes[..4]);

        let ct = content_type(&base, key);
        assert!(ct.starts_with("image/"), "contentType 应以 image/ 开头，实际 {ct}");
    }
}
