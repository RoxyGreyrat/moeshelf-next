/**
 * 展示层的数字格式化。
 *
 * 单独抽出来是因为「体积」和「数量」在两个地方出现（详情面板、设置页），
 * 各写一份必然出现 `1.2 GB` 与 `1.2GB` 这种不一致。
 */

/** 1536 → `1.5 KB`；0 → `0 B` */
export function formatBytes(n: number): string {
  if (!n || n < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = n;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i += 1;
  }
  // 字节数不需要小数；大于 100 的也留整数，避免 `1023.7 MB` 这种噪音
  const digits = i === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[i]}`;
}

/** 1204 → `1,204` */
export function formatCount(n: number): string {
  return (n ?? 0).toLocaleString("zh-CN");
}
