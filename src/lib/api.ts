import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Character, Game } from "@/types/game";

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

/* ---------------------------------------------------------------- C 阶段：扫描 */

/** 扫描过程中 Rust 侧推过来的进度（节流到约 10 帧/秒，见 commands.rs） */
export interface ScanProgress {
  /** 已处理的目录数（不是游戏数，目录多得多） */
  dirsScanned: number;
  /** 已识别出的游戏数 */
  gamesFound: number;
  /** 当前正在看的目录，结束时是空串 */
  current: string;
}

/** 一次扫描的最终结果 */
export interface ScanReport {
  root: string;
  dirsScanned: number;
  gamesFound: number;
  /** 新增进库的条数 */
  inserted: number;
  /** 库里已有、只刷新了扫描事实（路径/体积/主程序）的条数 */
  updated: number;
  elapsedMs: number;
}


/**
 * 扫描一个根目录并写库。
 *
 * 注意这个调用会持续几十秒到几分钟：扫描期间 Rust 侧每 100ms 发一次
 * `scan-progress` 事件，想显示进度就先调 `onScanProgress`。
 */
export function scanDirectory(root: string): Promise<ScanReport> {
  return invoke<ScanReport>("scan_directory", { root });
}

/** 订阅扫描进度。返回值是取消订阅函数，**必须**在组件卸载或扫描结束时调用。 */
export function onScanProgress(handler: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan-progress", (event) => handler(event.payload));
}

/* ---------------------------------------------------- C 阶段：忽略 / 删除 */

/**
 * 被忽略的条目。
 *
 * 忽略**不删行**：`games.ignored = 1`，`list_games` 会过滤掉，
 * 所以这里能捞回来让用户恢复。重扫同一目录也不会把它放出来。
 */
export function listIgnoredGames(): Promise<Game[]> {
  return invoke<Game[]>("list_ignored_games");
}

/** 忽略 / 取消忽略若干条。返回真正改动的行数（整批在一个事务里）。 */
export function setGamesIgnored(ids: string[], ignored: boolean): Promise<number> {
  return invoke<number>("set_games_ignored", { ids, ignored });
}

/**
 * 从库里彻底删除若干条（连带游玩记录）。
 *
 * ⚠️ 只删数据库：硬盘上的游戏目录、封面缓存都不动。
 * 删完再扫同一个目录，它会作为"新游戏"回来。
 */
export function deleteGames(ids: string[]): Promise<number> {
  return invoke<number>("delete_games", { ids });
}

/* ---------------------------------------------------- 扫描根目录 / 系统信息 */

/** 已登记的扫描根目录（顶栏扫描按钮会列出它们） */
export interface ScanRoot {
  id: number;
  path: string;
  /** 上次扫描完成时间，从没扫过是 null */
  lastScanAt: string | null;
  addedAt: string;
}

export function listScanRoots(): Promise<ScanRoot[]> {
  return invoke<ScanRoot[]>("list_scan_roots");
}

/** 登记一个根目录。目录不存在时后端会抛错（前端不用自己判断）。 */
export function addScanRoot(path: string): Promise<ScanRoot> {
  return invoke<ScanRoot>("add_scan_root", { path });
}

export function removeScanRoot(id: number): Promise<void> {
  return invoke<void>("remove_scan_root", { id });
}

/** 用资源管理器打开一个目录 */
export function openFolder(path: string): Promise<void> {
  return invoke<void>("open_folder", { path });
}

/** 在资源管理器里定位到某一项（打开父目录并选中它） */
export function revealPath(path: string): Promise<void> {
  return invoke<void>("reveal_path", { path });
}

/** 设置页「数据与关于」用的静态信息 */
export interface AppInfo {
  version: string;
  schemaVersion: number;
  dataDir: string;
  dbPath: string;
  dbSize: number;
  coverDir: string;
  coverDirExists: boolean;
  coverCount: number;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

/** 全屏详情页一次要用的数据（游戏 + 角色），避免分两次拉导致内容跳变 */
export interface GameDetail {
  game: Game;
  characters: Character[];
}

export function getGameDetail(id: string): Promise<GameDetail> {
  return invoke<GameDetail>("get_game_detail", { id });
}

/* ---------------------------------------------------- 自制路径选择器 */

export interface PathEntry {
  name: string;
  path: string;
  isDir: boolean;
}

export interface PathListing {
  /** 当前目录；null = 还没进入目录（只列盘符） */
  path: string | null;
  parent: string | null;
  entries: PathEntry[];
}

/** 列目录（不弹系统对话框，交给自制的选择器渲染） */
export function listPath(path?: string | null): Promise<PathListing> {
  return invoke<PathListing>("list_path", { path: path ?? null });
}

/* ---------------------------------------------------- 封面编辑 */

/** 用本地图片当封面（会复制进应用数据目录，之后删原图也不影响） */
export function setCoverFromFile(id: string, source: string): Promise<Game> {
  return invoke<Game>("set_cover_from_file", { id, source });
}

/** 用图片 URL 当封面（先下载到本地，断网也不会变裂图） */
export function setCoverFromUrl(id: string, url: string): Promise<Game> {
  return invoke<Game>("set_cover_from_url", { id, url });
}

/** 恢复默认封面（清空并解锁） */
export function resetCover(id: string): Promise<Game> {
  return invoke<Game>("reset_cover", { id });
}

/* ---------------------------------------------------- VNDB 刮削 */

export interface VnCandidate {
  id: string;
  title: string;
  altTitle: string;
  titleCn: string;
  imageUrl: string | null;
  released: string;
  /** 已归一 0~10 */
  rating: number;
  developer: string;
  description: string;
}

export function vndbSearch(query: string, limit = 10): Promise<VnCandidate[]> {
  return invoke<VnCandidate[]>("vndb_search", { query, limit });
}

export interface ScrapeReport {
  game: Game;
  updatedFields: string[];
  skippedLocked: string[];
  characters: number;
  coverSaved: boolean;
}

export function vndbApply(id: string, vndbId: string): Promise<ScrapeReport> {
  return invoke<ScrapeReport>("vndb_apply", { id, vndbId });
}
