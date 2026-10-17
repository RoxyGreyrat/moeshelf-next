import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { ALL_TAGS, SEED_GAMES } from "@/data/games";
import type {
  Engine,
  Filters,
  Game,
  GridState,
  Platform,
  SelectMode,
  ViewMode,
} from "@/types/game";
import { ENGINE_LABEL, PLATFORM_LABEL } from "@/types/game";

import { TopBar } from "@/components/layout/TopBar";
import { FilterSidebar } from "@/components/layout/FilterSidebar";
import { GameGrid } from "@/components/game/GameGrid";
import { DetailPanel } from "@/components/detail/DetailPanel";
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
  const [games, setGames] = useState<Game[]>(SEED_GAMES);
  const [query, setQuery] = useState("");
  const [view, setView] = useState<ViewMode>("grid");
  const [gridState, setGridState] = useState<GridState>("normal");
  const [selectMode, setSelectMode] = useState<SelectMode>("single");
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS);

  // 选中的游戏（多选时是集合，单选时只有一个）
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  // 键盘光标：方向键移动的是它，单选时与选中同步
  const [cursorId, setCursorId] = useState<string | null>(null);

  const [editing, setEditing] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [columns, setColumns] = useState(4);

  const searchRef = useRef<HTMLInputElement>(null);

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
    const base = Object.fromEntries(
      (Object.keys(ENGINE_LABEL) as Engine[]).map((e) => [e, 0]),
    ) as Record<Engine, number>;
    for (const g of games) base[g.engine] += 1;
    return base;
  }, [games]);

  const tagCounts = useMemo(() => {
    const map: Record<string, number> = {};
    for (const g of games)
      for (const t of g.tags) map[t] = (map[t] ?? 0) + 1;
    return map;
  }, [games]);

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

  const handleScan = useCallback(() => {
    toast.success("扫描完成", {
      description: "在 E:\\galgame 下发现 12 部游戏，其中 3 部引擎未识别。",
    });
  }, []);

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
    (patch: Partial<Game>) => {
      if (!activeGame) return;
      const keys = Object.keys(patch);
      if (keys.length === 0) {
        setEditing(false);
        toast("没有检测到改动");
        return;
      }
      setGames((prev) =>
        prev.map((g) =>
          g.id === activeGame.id
            ? {
                ...g,
                ...patch,
                lockedFields: Array.from(
                  new Set([...g.lockedFields, ...keys]),
                ),
              }
            : g,
        ),
      );
      setEditing(false);
      toast.success(`已保存 ${keys.length} 个字段`, {
        description: "这些字段已被锁定，下次刮削不会覆盖。",
      });
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
      <div className="flex h-full flex-col overflow-hidden bg-background text-foreground">
        <TopBar
          query={query}
          onQueryChange={setQuery}
          view={view}
          onViewChange={setView}
          onScan={handleScan}
          onOpenSettings={() =>
            toast("设置面板将在 B 阶段接入", {
              description: "扫描根目录、代理、刮削源等配置。",
            })
          }
          searchRef={searchRef}
        />

        <div className="flex min-h-0 flex-1">
          <FilterSidebar
            filters={filters}
            platformCounts={platformCounts}
            engineCounts={engineCounts}
            tagCounts={tagCounts}
            allTags={ALL_TAGS}
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
                  "正在扫描目录…"
                ) : gridState === "empty" ? (
                  "书架为空"
                ) : (
                  "加载失败"
                )}
              </p>
              <p className="text-[11px] text-muted-foreground/80">
                Ctrl K 搜索 · 方向键移动 · Enter 打开 · Esc 关闭
              </p>
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
            playing={playing}
            onStartEdit={() => setEditing(true)}
            onCancelEdit={() => setEditing(false)}
            onSave={handleSave}
            onPlay={handlePlay}
            onPatch={handlePatch}
          />
        </div>

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
