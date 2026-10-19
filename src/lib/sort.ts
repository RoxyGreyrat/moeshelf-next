import type { Game, SortState } from "@/types/game";

/**
 * 显示用的标题：中文 → 外文 → 文件夹名（与卡片上的规则保持一致）。
 * 排序必须用**显示出来的那个名字**，否则用户会看到"A 打头的卡片排在 B 后面"。
 */
export function displayTitle(g: Game): string {
  return (
    g.titleCn?.trim() ||
    g.title?.trim() ||
    g.originalTitle?.trim() ||
    folderName(g)
  );
}

function folderName(g: Game): string {
  const parts = (g.folderPath ?? "").split(/[\\/]+/).filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : "";
}

/**
 * 排序。
 *
 * 两个刻意的设计：
 * 1. **空值永远沉底** —— 没有发售日的游戏不该因为"正序/倒序"在列表顶上乱晃；
 * 2. **同值时按标题兜底** —— 评分相同的几部不会每次刷新都换位置（排序要稳定）。
 *
 * 返回新数组，不改原数组：`games` 是 React 状态，就地 sort 会连带的 UI 一起变。
 */
export function sortGames(games: Game[], sort: SortState): Game[] {
  const dir = sort.dir === "asc" ? 1 : -1;

  const value = (g: Game): string | number | null => {
    switch (sort.key) {
      case "released":
        return g.released?.trim() || null;
      case "title":
        return displayTitle(g);
      case "rating":
        return typeof g.rating === "number" ? g.rating : null;
      case "added":
        return g.firstSeenAt?.trim() || null;
    }
  };

  return [...games].sort((a, b) => {
    const va = value(a);
    const vb = value(b);

    // 空值沉底：与正序/倒序无关
    if (va === null && vb === null) return compareTitle(a, b);
    if (va === null) return 1;
    if (vb === null) return -1;

    let cmp: number;
    if (typeof va === "number" && typeof vb === "number") {
      cmp = va - vb;
    } else if (sort.key === "title") {
      // 中文按拼音、数字按数值（"9-nine" 要排在 "10" 前面）
      cmp = String(va).localeCompare(String(vb), "zh-Hans-CN", {
        numeric: true,
        sensitivity: "base",
      });
    } else {
      // 发售日 / 入库时间都是 ISO 字符串，直接字典序比较即可
      cmp = String(va) < String(vb) ? -1 : String(va) > String(vb) ? 1 : 0;
    }

    if (cmp === 0) return compareTitle(a, b);
    return cmp * dir;
  });
}

function compareTitle(a: Game, b: Game): number {
  return displayTitle(a).localeCompare(displayTitle(b), "zh-Hans-CN", {
    numeric: true,
    sensitivity: "base",
  });
}
