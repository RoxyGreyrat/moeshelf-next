/**
 * 新数据模型（第 2 步与产品负责人敲定的字段）
 * ⚠️ 第 B 阶段接真库时，这里应该由 Rust 命令层的返回值替换，字段名保持一对一。
 */

export type Platform = "windows" | "android";

export type Engine = "kirikiri" | "siglus" | "artemis" | "bgi" | "renpy" | "unity";

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
  /** 假封面用的双色（真库会换成 coverUrl） */
  coverColors: [string, string];
  /** 主程序：旧库 exeMap 提升成列 */
  launchExe: string;
  folderPath: string;
  fileCount: number;
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
};

export type GridState = "normal" | "loading" | "empty" | "error";

export type SelectMode = "single" | "multi";

export type ViewMode = "grid" | "list";

export interface Filters {
  platforms: Platform[];
  engines: Engine[];
  tags: string[];
}
