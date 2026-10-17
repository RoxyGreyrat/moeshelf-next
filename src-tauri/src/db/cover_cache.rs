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
