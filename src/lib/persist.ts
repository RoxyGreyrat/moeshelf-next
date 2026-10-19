import type { Filters, SelectMode, SortKey, SortState, ViewMode } from "@/types/game";

/**
 * 界面状态的本地持久化。
 *
 * 目标只有一个：**关掉再打开，看到的东西和上次一样**。
 * 存的是"用户的选择"（视图 / 排序 / 筛选 / 搜索词 / 选中项），
 * 不存任何游戏数据 —— 那些以数据库为准。
 *
 * 用 localStorage 而不是数据库：这是纯前端偏好，不需要参与同步、迁移、备份；
 * 写坏了最多回到默认值，不会影响书架。
 */
const UI_KEY = "moeshelf.ui.v1";
const GROUP_KEY = "moeshelf.filterGroups.v1";

const SORT_KEYS: SortKey[] = ["released", "title", "rating", "added"];

export interface PersistedUi {
  view: ViewMode;
  sort: SortState;
  filters: Filters;
  query: string;
  selectMode: SelectMode;
  selectedIds: string[];
  cursorId: string | null;
}

export function loadUi(): Partial<PersistedUi> {
  try {
    const raw = localStorage.getItem(UI_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    return parsed && typeof parsed === "object" ? (parsed as Partial<PersistedUi>) : {};
  } catch {
    // 手动改坏了 / 隐私模式禁止存储：当作没有存过
    return {};
  }
}

export function saveUi(patch: Partial<PersistedUi>): void {
  try {
    localStorage.setItem(UI_KEY, JSON.stringify({ ...loadUi(), ...patch }));
  } catch {
    // 存不进去不影响功能
  }
}

/** 读出来之后先过一遍校验：版本升级/手改都可能留下不认识的值 */
export function sanitizeUi(ui: Partial<PersistedUi>): {
  view: ViewMode;
  sort: SortState;
  filters: Filters;
  query: string;
  selectMode: SelectMode;
  selectedIds: string[];
  cursorId: string | null;
} {
  const view: ViewMode = ui.view === "list" ? "list" : "grid";

  const sort: SortState =
    ui.sort && SORT_KEYS.includes(ui.sort.key) && (ui.sort.dir === "asc" || ui.sort.dir === "desc")
      ? { key: ui.sort.key, dir: ui.sort.dir }
      : { key: "released", dir: "desc" };

  const list = (v: unknown): string[] =>
    Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];

  const filters: Filters = {
    platforms: list(ui.filters?.platforms) as Filters["platforms"],
    developers: list(ui.filters?.developers),
    engines: list(ui.filters?.engines) as Filters["engines"],
    tags: list(ui.filters?.tags),
  };

  return {
    view,
    sort,
    filters,
    query: typeof ui.query === "string" ? ui.query : "",
    selectMode: ui.selectMode === "multi" ? "multi" : "single",
    selectedIds: list(ui.selectedIds),
    cursorId: typeof ui.cursorId === "string" ? ui.cursorId : null,
  };
}

/** 筛选分组的展开/收起（按分组标题记） */
export function loadGroupOpen(): Record<string, boolean> {
  try {
    const raw = localStorage.getItem(GROUP_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object") return {};
    const out: Record<string, boolean> = {};
    for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof v === "boolean") out[k] = v;
    }
    return out;
  } catch {
    return {};
  }
}

export function saveGroupOpen(title: string, open: boolean): void {
  try {
    localStorage.setItem(
      GROUP_KEY,
      JSON.stringify({ ...loadGroupOpen(), [title]: open }),
    );
  } catch {
    // ignore
  }
}

/** 左侧筛选栏是否收起（"1" = 收起） */
const SIDEBAR_KEY = "moeshelf.sidebarCollapsed";

export function loadSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_KEY) === "1";
  } catch {
    return false;
  }
}

export function saveSidebarCollapsed(collapsed: boolean): void {
  try {
    localStorage.setItem(SIDEBAR_KEY, collapsed ? "1" : "0");
  } catch {
    // ignore
  }
}
