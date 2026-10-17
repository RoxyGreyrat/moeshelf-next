import { useCallback, useEffect, useMemo, useRef, useState, useTransition } from "react";
import { toast } from "sonner";

import {
  deleteGames,
  listGames,
  onScanProgress,
  pickFolder,
  scanDirectory,
  setGamesIgnored,
  updateGame,
} from "@/lib/api";
import type { ScanProgress } from "@/lib/api";
import type {
  Filters,
  Game,
  GridState,
  Platform,
  SelectMode,
  ViewMode,
} from "@/types/game";
import { PLATFORM_LABEL } from "@/types/game";

import { TopBar } from "@/components/layout/TopBar";
import { FilterSidebar } from "@/components/layout/FilterSidebar";
import { ScanStatus } from "@/components/layout/ScanStatus";
import { GameGrid } from "@/components/game/GameGrid";
import { DetailPanel } from "@/components/detail/DetailPanel";
import { DeleteGamesDialog } from "@/components/detail/DeleteGamesDialog";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { StateSwitcher } from "@/components/debug/StateSwitcher";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";

const EMPTY_FILTERS: Filters = { platforms: [], engines: [], tags: [] };

function toggleIn<T>(list: T[], value: T): T[] {
  return list.includes(value)
    ? list.filter((v) => v !== value)
    : [...list, value];
}

export default function App() {
  // 数据来自 Rust 命令层：启动时 list_games，失败则停在错误态
  const [games, setGames] = useState<Game[]>([]);
  const [query, setQuery] = useState("");
  const [view, setView] = useState<ViewMode>("grid");
  const [gridState, setGridState] = useState<GridState>("loading");
  const [selectMode, setSelectMode] = useState<SelectMode>("single");
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS);

  // 选中的游戏（多选时是集合，单选时只有一个）
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  // 键盘光标：方向键移动的是它，单选时与选中同步
  const [cursorId, setCursorId] = useState<string | null>(null);

  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [columns, setColumns] = useState(4);

  // 扫描：scanning 只控制顶栏按钮状态，scanProgress 是 Rust 每 100ms 推过来的最新进度
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);

  // 设置面板 / 删除确认（删除是个不可撤销的动作，必须二次确认）
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Game[]>([]);
  const [deleteBusy, setDeleteBusy] = useState(false);

  // 切视图会重建几十个卡片子树：走并发更新，让浏览器先响应点击再提交列表
  const [, startTransition] = useTransition();

  const searchRef = useRef<HTMLInputElement>(null);

  /* ------------------------------------------------------------ 数据装载（Rust） */

  /** 从 SQLite 读全量列表。成功后清掉选中，避免指向已不存在的 id */
  const reload = useCallback(async () => {
    try {
      const rows = await listGames();
      setGames(rows);
      setSelectedIds([]);
      setCursorId(null);
      setGridState("normal");
      return rows;
    } catch (err) {
      console.error("[list_games]", err);
      setGridState("error");
      toast.error("读取游戏库失败", {
        description: String(err),
      });
      return [];
    }
  }, []);

  // 首帧就是骨架屏，读库完成后切回 normal
  useEffect(() => {
    void reload();
  }, [reload]);

  /* ---------------------------------------------------------------- 派生数据 */

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return games.filter((g) => {
      if (filters.platforms.length && !filters.platforms.includes(g.platform))
        return false;
      if (filters.engines.length && !filters.engines.includes(g.engine))
        return false;
      if (filters.tags.length && !filters.tags.every((t) => g.tags.includes(t)))
        return false;
      if (!q) return true;
      const haystack = [
        g.titleCn,
        g.title,
        g.originalTitle,
        g.developer,
        g.tags.join(" "),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(q);
    });
  }, [games, filters, query]);

  const activeGame = useMemo(() => {
    const id = selectedIds[selectedIds.length - 1];
    if (!id) return null;
    return games.find((g) => g.id === id) ?? null;
  }, [games, selectedIds]);

  const activeCount =
    filters.platforms.length + filters.engines.length + filters.tags.length;

  const platformCounts = useMemo(() => {
    const base = Object.fromEntries(
      (Object.keys(PLATFORM_LABEL) as Platform[]).map((p) => [p, 0]),
    ) as Record<Platform, number>;
    for (const g of games) base[g.platform] += 1;
    return base;
  }, [games]);

  const engineCounts = useMemo(() => {
    // 只统计库里真实出现的引擎：真实数据里会有未登记的新引擎（如 catsystem2）
    const map: Record<string, number> = {};
    for (const g of games) map[g.engine] = (map[g.engine] ?? 0) + 1;
    return map;
  }, [games]);

  const tagCounts = useMemo(() => {
    const map: Record<string, number> = {};
    for (const g of games)
      for (const t of g.tags) map[t] = (map[t] ?? 0) + 1;
    return map;
  }, [games]);

  /** 标签候选项不再手写，直接由库里的数据汇总 */
  const allTags = useMemo(
    () => Array.from(new Set(games.flatMap((g) => g.tags))).sort(),
    [games],
  );

  /* ------------------------------------------------------------------ 交互 */

  const clearFilters = useCallback(() => {
    setFilters(EMPTY_FILTERS);
    setQuery("");
  }, []);

  const handleSelect = useCallback(
    (id: string) => {
      setCursorId(id);
      setEditing(false);
      if (selectMode === "single") {
        setSelectedIds([id]);
        return;
      }
      setSelectedIds((prev) => toggleIn(prev, id));
    },
    [selectMode],
  );

  const handleOpen = useCallback((id: string) => {
    setCursorId(id);
    setSelectedIds([id]);
    setEditing(false);
  }, []);

  /**
   * 扫描：选目录 → 订阅进度 → Rust 扫完写库 → 重拉列表 → 报数字。
   *
   * 扫描在 Rust 侧跑在 `spawn_blocking` 里，几分钟也不能卡住界面：
   * 进度走事件推送，最终结果走命令返回值。
   */
  const handleScan = useCallback(async () => {
    if (scanning) return;

    let root: string | null;
    try {
      root = await pickFolder("选择游戏库根目录");
    } catch (err) {
      console.error("[pick_folder]", err);
      toast.error("打不开文件夹选择框", { description: String(err) });
      return;
    }
    if (!root) return; // 用户取消，什么都不做

    setScanning(true);
    setScanProgress({ dirsScanned: 0, gamesFound: 0, current: "" });

    // 先订阅再调用：Rust 侧一开扫就发事件，订阅晚了会漏掉开头几帧
    const unlisten = await onScanProgress(setScanProgress);

    try {
      const report = await scanDirectory(root);
      // 扫描前选中的那条可能已经不在列表里了，重拉一次最省事
      await reload();

      const seconds = (report.elapsedMs / 1000).toFixed(1);
      toast.success(
        report.inserted > 0
          ? `扫描完成，新增 ${report.inserted} 部`
          : "扫描完成，没有新游戏",
        {
          description: `识别 ${report.gamesFound} 部 · 更新 ${report.updated} 部 · 看了 ${report.dirsScanned} 个文件夹 · 用时 ${seconds} 秒`,
        },
      );
    } catch (err) {
      console.error("[scan_directory]", err);
      // 不动 gridState：库里原有的数据还是好的，只是这次扫描失败了
      toast.error("扫描失败", { description: String(err) });
    } finally {
      unlisten();
      setScanning(false);
      setScanProgress(null);
    }
  }, [reload, scanning]);

  /* ------------------------------------------------------ 忽略 / 删除（库维护） */

  /** 当前作用对象：多选是选中的全部，单选就是详情里那一部 */
  const targetIds = useCallback(() => {
    if (selectMode === "multi" && selectedIds.length > 0) return selectedIds;
    return activeGame ? [activeGame.id] : [];
  }, [activeGame, selectMode, selectedIds]);

  /** 从本地列表摘掉若干条并把选中/编辑态收干净。不整表重拉 —— 摘掉一条没必要刷新 211 条 */
  const dropLocally = useCallback((ids: string[]) => {
    setGames((prev) => prev.filter((g) => !ids.includes(g.id)));
    setSelectedIds([]);
    setCursorId(null);
    setEditing(false);
  }, []);

  /** 忽略：行还在库里（ignored=1），只是书架不再显示，设置里能恢复 */
  const handleIgnore = useCallback(async () => {
    const ids = targetIds();
    if (ids.length === 0) return;
    const name = activeGame?.titleCn ?? "";

    try {
      const n = await setGamesIgnored(ids, true);
      dropLocally(ids);
      toast.success(ids.length > 1 ? `已忽略 ${n} 部` : `已忽略《${name}》`, {
        description: "在右上角「设置 → 已忽略」里可以恢复。",
      });
    } catch (err) {
      console.error("[set_games_ignored]", err);
      toast.error("忽略失败", { description: String(err) });
    }
  }, [activeGame, dropLocally, targetIds]);

  const handleDeleteRequest = useCallback(() => {
    const ids = targetIds();
    if (ids.length === 0) return;
    setDeleteTarget(games.filter((g) => ids.includes(g.id)));
  }, [games, targetIds]);

  /** 删除：确认后从库里删记录（连带游玩记录），硬盘文件不动 */
  const handleDeleteConfirm = useCallback(async () => {
    const ids = deleteTarget.map((g) => g.id);
    if (ids.length === 0) return;

    setDeleteBusy(true);
    try {
      const n = await deleteGames(ids);
      dropLocally(ids);
      setDeleteTarget([]);
      toast.success(`已从书架删除 ${n} 部`, {
        description: "硬盘上的游戏文件没有动；重新扫描这个目录它会再出现。",
      });
    } catch (err) {
      console.error("[delete_games]", err);
      // 失败就保持弹窗开着，让用户能重试
      toast.error("删除失败", { description: String(err) });
    } finally {
      setDeleteBusy(false);
    }
  }, [deleteTarget, dropLocally]);

  const handlePatch = useCallback(() => {
    if (!activeGame) return;
    toast.success(`已打开《${activeGame.titleCn}》的补丁目录`, {
      description: "本阶段为演示入口，D 阶段接入真实补丁管理。",
    });
  }, [activeGame]);

  const handlePlay = useCallback(() => {
    if (!activeGame) return;
    const name = activeGame.titleCn;
    setPlaying(true);
    // 演示：模拟一次进程启动，1.4 秒后回到可点状态
    window.setTimeout(() => {
      setPlaying(false);
      toast.success(`正在启动《${name}》`, {
        description: activeGame.launchExe,
      });
    }, 1400);
  }, [activeGame]);

  const handleSave = useCallback(
    async (patch: Partial<Game>) => {
      if (!activeGame) return;
      const keys = Object.keys(patch);
      if (keys.length === 0) {
        setEditing(false);
        toast("没有检测到改动");
        return;
      }

      setSaving(true);
      try {
        // 真正落库：返回的是 UPDATE 之后从库里重新 SELECT 出来的整行
        const updated = await updateGame(activeGame.id, patch);
        // 只替换这一条，不整表重拉 —— 重拉会清空选中，详情面板会跟着关掉
        setGames((prev) => prev.map((g) => (g.id === updated.id ? updated : g)));
        setEditing(false);
        toast.success(`已保存 ${keys.length} 个字段`, {
          description: `已写入数据库，${keys.length} 个字段已锁定，刮削不会覆盖。`,
        });
      } catch (err) {
        console.error("[update_game]", err);
        // 保存失败保持编辑态，避免用户输入被吞掉
        toast.error("保存失败", { description: String(err) });
      } finally {
        setSaving(false);
      }
    },
    [activeGame],
  );

  /* --------------------------------------------------------------- 键盘操作 */

  const moveCursor = useCallback(
    (delta: number) => {
      if (gridState !== "normal" || filtered.length === 0) return;
      const current = cursorId
        ? filtered.findIndex((g) => g.id === cursorId)
        : -1;
      const base = current >= 0 ? current : 0;
      const next = Math.max(0, Math.min(filtered.length - 1, base + delta));
      const id = filtered[next].id;

      setCursorId(id);
      if (selectMode === "single") setSelectedIds([id]);

      requestAnimationFrame(() => {
        document
          .querySelector(`[data-card-id="${id}"]`)
          ?.scrollIntoView({ block: "nearest" });
      });
    },
    [cursorId, filtered, gridState, selectMode],
  );

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing =
        !!target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      // Ctrl/Cmd + K：聚焦搜索框
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }

      // Esc：优先退出编辑 → 再清空详情 → 最后让输入框失焦
      if (e.key === "Escape") {
        if (editing) {
          setEditing(false);
          return;
        }
        if (selectedIds.length > 0) {
          setSelectedIds([]);
          setCursorId(null);
          return;
        }
        if (typing) target?.blur();
        return;
      }

      if (typing) return;
      if (gridState !== "normal") return;

      switch (e.key) {
        case "ArrowLeft":
          e.preventDefault();
          moveCursor(-1);
          break;
        case "ArrowRight":
          e.preventDefault();
          moveCursor(1);
          break;
        case "ArrowUp":
          e.preventDefault();
          moveCursor(-columns);
          break;
        case "ArrowDown":
          e.preventDefault();
          moveCursor(columns);
          break;
        case "Enter":
          if (cursorId) {
            e.preventDefault();
            handleOpen(cursorId);
          }
          break;
        case " ":
          if (selectMode === "multi" && cursorId) {
            e.preventDefault();
            setSelectedIds((prev) => toggleIn(prev, cursorId));
          }
          break;
        default:
          break;
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    columns,
    cursorId,
    editing,
    gridState,
    handleOpen,
    moveCursor,
    selectMode,
    selectedIds.length,
  ]);

  /* -------------------------------------------------------------- 渲染 */

  return (
    <TooltipProvider delayDuration={300}>
      <div className="relative flex h-full flex-col overflow-hidden bg-background text-foreground">
        <TopBar
          query={query}
          onQueryChange={setQuery}
          view={view}
          onViewChange={(v) => startTransition(() => setView(v))}
          onScan={handleScan}
          scanning={scanning}
          onOpenSettings={() => setSettingsOpen(true)}
          searchRef={searchRef}
        />

        <div className="flex min-h-0 flex-1">
          <FilterSidebar
            filters={filters}
            platformCounts={platformCounts}
            engineCounts={engineCounts}
            tagCounts={tagCounts}
            allTags={allTags}
            activeCount={activeCount}
            onTogglePlatform={(p) =>
              setFilters((f) => ({ ...f, platforms: toggleIn(f.platforms, p) }))
            }
            onToggleEngine={(e) =>
              setFilters((f) => ({ ...f, engines: toggleIn(f.engines, e) }))
            }
            onToggleTag={(t) =>
              setFilters((f) => ({ ...f, tags: toggleIn(f.tags, t) }))
            }
            onClearAll={clearFilters}
          />

          <main className="flex min-w-0 flex-1 flex-col">
            <div className="flex flex-wrap items-baseline justify-between gap-2 px-6 pb-2 pt-4">
              <p className="text-[12px] text-muted-foreground">
                {gridState === "normal" ? (
                  <>
                    共 <span className="font-medium text-foreground">{filtered.length}</span> 部
                    {selectMode === "multi" && selectedIds.length > 0 && (
                      <span className="ml-2 text-accent-foreground">
                        · 已选 {selectedIds.length} 部
                      </span>
                    )}
                  </>
                ) : gridState === "loading" ? (
                  "正在读取书架…"
                ) : gridState === "empty" ? (
                  "书架为空"
                ) : (
                  "加载失败"
                )}
              </p>
              {/* 扫描中把快捷键提示换成进度：同一行、同一位置，不遮挡封面也不让内容跳动 */}
              {scanProgress ? (
                <ScanStatus progress={scanProgress} />
              ) : (
                <p className="text-[11px] text-muted-foreground/80">
                  Ctrl K 搜索 · 方向键移动 · Enter 打开 · Esc 关闭
                </p>
              )}
            </div>

            {/* pt-6 是必须的：卡片悬停会上浮 4px + 放大 1.04（顶部再外扩约 4.5px），
                选中外框还额外向外 3px。滚动容器没有顶部内边距时，这 8~9px 会被直接切平。
                需要 ≥10px 的呼吸空间，这里给 24px。 */}
            <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-44 pt-6">
              <GameGrid
                games={filtered}
                view={view}
                state={gridState}
                selectedIds={selectedIds}
                cursorId={cursorId}
                multi={selectMode === "multi"}
                onSelect={handleSelect}
                onOpen={handleOpen}
                onScan={handleScan}
                onRetry={() => setGridState("normal")}
                onClearFilters={clearFilters}
                onColumnsChange={setColumns}
              />
            </div>
          </main>

          <DetailPanel
            game={activeGame}
            multiCount={selectMode === "multi" ? selectedIds.length : 1}
            editing={editing}
            saving={saving}
            playing={playing}
            onStartEdit={() => setEditing(true)}
            onCancelEdit={() => setEditing(false)}
            onSave={handleSave}
            onPlay={handlePlay}
            onPatch={handlePatch}
            onIgnore={handleIgnore}
            onDelete={handleDeleteRequest}
          />
        </div>

        <SettingsDialog
          open={settingsOpen}
          onOpenChange={setSettingsOpen}
          onRestored={() => void reload()}
        />

        <DeleteGamesDialog
          open={deleteTarget.length > 0}
          games={deleteTarget}
          busy={deleteBusy}
          onOpenChange={(next) => {
            if (!next) setDeleteTarget([]);
          }}
          onConfirm={handleDeleteConfirm}
        />

        <StateSwitcher
          gridState={gridState}
          onGridStateChange={setGridState}
          selectMode={selectMode}
          onSelectModeChange={setSelectMode}
          editing={editing}
          onEditingChange={setEditing}
          onToastSuccess={() =>
            toast.success("刮削成功", { description: "VNDB 已匹配 v19273 · 樱之诗" })
          }
          onToastError={() =>
            toast.error("刮削失败", { description: "请求超时，请检查代理设置后重试。" })
          }
        />

        <Toaster position="top-center" />
      </div>
    </TooltipProvider>
  );
}
