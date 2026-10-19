/**
 * 新数据模型（第 2 步与产品负责人敲定的字段）
 * ⚠️ 第 B 阶段接真库时，这里应该由 Rust 命令层的返回值替换，字段名保持一对一。
 */

export type Platform = "windows" | "android";

export type Engine =
  | "kirikiri"
  | "siglus"
  | "artemis"
  | "bgi"
  | "renpy"
  | "unity"
  | "catsystem2"
  /** 旧库真实数据里出现过、但还没做指纹识别的引擎 */
  | "yuris"
  | "advhd"
  /** 扫描器认不出来的引擎。真实数据里一定有，不能省 */
  | "unknown";

export type RatingSource = "vndb" | "bangumi";

export interface Game {
  /** 稳定主键：沿用旧库 djb2 路径哈希（8 位十六进制） */
  id: string;
  pathHash: string;
  /** 罗马字 / 英文标题 */
  title: string;
  /** 中文标题：officialCnTitle 优先，无则 cnTitle */
  titleCn: string;
  /** 日文原名 */
  originalTitle: string;
  developer: string;
  /** YYYY-MM-DD */
  released: string;
  /** 引擎：从旧库 matched_types 的 engine:xxx 提取，独立成列 */
  engine: Engine;
  platform: Platform;
  tags: string[];
  /** 主评分（0-10） */
  rating: number;
  ratingSource: RatingSource;
  description: string;
  cnDescription?: string;
  /**
   * 假封面用的双色。真库里的记录没有这个字段（封面走 coverUrl + 本地缓存），
   * 缺省时 CoverArt 会按 id 哈希从固定色板里挑一组，保证同一部游戏每次颜色一致。
   */
  coverColors?: [string, string];
  /** 封面远程直链（VNDB CDN 等）。B 阶段后续接入本地缓存后改用它 */
  coverUrl?: string | null;
  /** 主程序：旧库 exeMap 提升成列 */
  launchExe: string;
  folderPath: string;
  fileCount: number;
  /** 目录总字节数（递归统计）。旧数据为 0（V2 才加的列） */
  dirSize: number;
  matchScore: number;
  completed: boolean;
  /** ISO 字符串 */
  firstSeenAt: string;
  /** 元数据锁定字段，替代旧库 7 张 Map */
  lockedFields: string[];
  vndbId?: string;
}

export const PLATFORM_LABEL: Record<Platform, string> = {
  windows: "Windows",
  android: "Android",
};

export const ENGINE_LABEL: Record<Engine, string> = {
  kirikiri: "Kirikiri",
  siglus: "Siglus",
  artemis: "Artemis",
  bgi: "BGI",
  renpy: "Ren'Py",
  unity: "Unity",
  catsystem2: "CatSystem2",
  yuris: "YURIS",
  advhd: "AdvHD",
  unknown: "未识别",
};

/**
 * 引擎显示名。数据库里可能出现这里没登记的新引擎（真实数据里已经出现过 catsystem2），
 * 直接查表会得到 undefined、界面显示空白 —— 所以一律走这个函数，取不到就原样显示。
 */
export function engineLabel(engine: string): string {
  return (ENGINE_LABEL as Record<string, string>)[engine] ?? engine;
}

export type GridState = "normal" | "loading" | "empty" | "error";

export type SelectMode = "single" | "multi";

export type ViewMode = "grid" | "list";

export interface Filters {
  platforms: Platform[];
  engines: Engine[];
  tags: string[];
}

/**
 * 一个角色。
 *
 * 数据来自旧库缓存：VNDB 提供角色名/头像/定位，Bangumi 与 cngal 补声优。
 * `cv` 为空串表示"没收录"，不是"没有声优"。
 */
export interface Character {
  name: string;
  aliases: string[];
  /** 主角 / 主要人物 / 次要人物 / 仅出现…（旧版已本地化） */
  role: string;
  cv: string;
  /** 声优来源：bangumi / cngal / vndb */
  cvSource: string;
  imageUrl: string | null;
  sortOrder: number;
}
