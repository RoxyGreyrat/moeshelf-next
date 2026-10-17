import { invoke } from "@tauri-apps/api/core";
import type { Game } from "@/types/game";

/**
 * 前端与 Rust 命令层的唯一出口。
 * 组件里不要直接写 invoke("xxx")，统一走这里，方便以后加日志/超时/错误兜底。
 */

/** 读取全部游戏。字段名与 Rust 侧 `#[serde(rename_all = "camelCase")]` 对应 */
export function listGames(): Promise<Game[]> {
  return invoke<Game[]>("list_games");
}

/**
 * 一次编辑提交的增量补丁：**只放改动过的字段**。
 * 缺省的字段后端不会碰，"整行覆盖"会把这些字段冲成空值。
 */
export type GamePatch = Partial<
  Pick<
    Game,
    | "title"
    | "titleCn"
    | "originalTitle"
    | "developer"
    | "released"
    | "engine"
    | "platform"
    | "tags"
    | "rating"
    | "ratingSource"
    | "description"
    | "cnDescription"
    | "coverUrl"
    | "launchExe"
    | "completed"
    | "vndbId"
  >
>;

/**
 * 保存一次编辑。返回更新后从库里重新读出来的整行。
 *
 * `lock` 默认 true：改过的字段会并入 `locked_fields`，界面上显示小锁，
 * 以后刮削不会覆盖人工修改。
 */
export function updateGame(
  id: string,
  patch: GamePatch,
  lock = true,
): Promise<Game> {
  return invoke<Game>("update_game", { id, patch, lock });
}

/** 旧库默认位置（MoeShelf 2.1.0 活库，与 Rust 侧 DEFAULT_LEGACY_DIR 保持一致） */
export const DEFAULT_LEGACY_DIR = String.raw`E:\moeshelf\moeshelf-build-2.1.0\data`;

export interface MigrateReport {
  legacyDir: string;
  snapshotDir: string;
  gamesScanned: number;
  skippedFixture: number;
  imported: number;
  withMetadata: number;
  withoutMetadata: number;
  playtimeImported: number;
  overridesApplied: number;
  warnings: string[];
}

/** 从旧库一次性迁移（旧库只读；全部 INSERT OR REPLACE，可重复执行） */
export function migrateLegacy(legacyDir?: string): Promise<MigrateReport> {
  return invoke<MigrateReport>("migrate_legacy", { legacyDir });
}
