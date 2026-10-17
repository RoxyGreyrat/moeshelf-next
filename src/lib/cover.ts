/**
 * 封面 URL 解析。
 *
 * 旧库的封面缓存文件名 = djb2(原始 URL) 的 8 位十六进制，必须与 Rust
 * `PathHash` / 旧项目 `pathHashOf` 逐位一致，否则找不到文件。
 *
 * 缓存文件没有扩展名，所以不能走 convertFileSrc；改由 Rust 注册的
 * `cover` 自定义协议读取（自带 Content-Type）。Windows 上 Tauri 把
 * 自定义协议映射成 `http://<scheme>.localhost/<path>`。
 */

/** djb2：h = h*33 + c，逐位取模 2^32，与旧项目 PathHash.Of 等价 */
export function djb2Hex(input: string): string {
  let hash = 5381;
  for (let i = 0; i < input.length; i += 1) {
    // >>> 0 把结果压回 uint32；每一步都压，避免 JS double 精度问题
    hash = ((hash << 5) + hash + input.charCodeAt(i)) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

const IS_WINDOWS =
  typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

/** 由封面 URL 得到可放进 <img src> / <image href> 的地址；没有 URL 返回 null */
export function coverSrc(coverUrl?: string | null): string | null {
  const url = coverUrl?.trim();
  if (!url) return null;
  const key = djb2Hex(url);
  return IS_WINDOWS
    ? `http://cover.localhost/${key}`
    : `cover://localhost/${key}`;
}

/** 缓存键（调试用） */
export function coverKey(coverUrl?: string | null): string | null {
  const url = coverUrl?.trim();
  return url ? djb2Hex(url) : null;
}
