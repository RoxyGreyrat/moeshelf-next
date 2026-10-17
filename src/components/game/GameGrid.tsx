import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, FolderSearch, RotateCw, ScanLine } from "lucide-react";
import type { Game, GridState, ViewMode } from "@/types/game";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { GameItem } from "./GameItem";

interface GameGridProps {
  games: Game[];
  view: ViewMode;
  state: GridState;
  selectedIds: string[];
  cursorId: string | null;
  multi: boolean;
  onSelect: (id: string) => void;
  onOpen: (id: string) => void;
  onScan: () => void;
  onRetry: () => void;
  onClearFilters: () => void;
  /** 把当前列数回报给上层，方向键上下移动要用 */
  onColumnsChange: (cols: number) => void;
}

/** 状态面板的进出场：短、干脆，不抢戏 */
const panelMotion = {
  initial: { opacity: 0, y: 10 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -8 },
  transition: { duration: 0.18 },
};

/**
 * 性能阈值：
 * - 超过 120 条就不做逐项 layout 投影（211 个元素逐帧算包围盒会明显掉帧），
 *   改用"整块淡入"过渡，观感依然连续，但主线程几乎零成本。
 * - 只有前 36 条播入场动画，剩下的直接静态渲染，避免 211 个弹簧同时启动。
 */
const LAYOUT_ANIM_LIMIT = 120;
const ENTRANCE_LIMIT = 36;

/**
 * 分页渲染：一次只渲染 60 个真实节点，滚到底再追加。
 * 200+ 部游戏时，切视图要一次性卸载/重建所有卡片子树，实测单帧 200ms；
 * 限制在 60 个之后，切换成本回到可接受范围。
 */
const PAGE_SIZE = 40;

function StatePanel({
  icon,
  title,
  desc,
  action,
}: {
  icon: React.ReactNode;
  title: string;
  desc: string;
  action: React.ReactNode;
}) {
  return (
    <div className="flex h-full min-h-[420px] flex-col items-center justify-center gap-4 px-6 text-center">
      <div className="flex size-14 items-center justify-center rounded-2xl border border-border bg-card shadow-[0_1px_2px_rgba(24,24,27,0.04)]">
        {icon}
      </div>
      <div className="space-y-1.5">
        <p className="text-[15px] font-medium text-foreground">{title}</p>
        <p className="mx-auto max-w-[280px] text-[13px] leading-relaxed text-muted-foreground">
          {desc}
        </p>
      </div>
      {action}
    </div>
  );
}

export function GameGrid({
  games,
  view,
  state,
  selectedIds,
  cursorId,
  multi,
  onSelect,
  onOpen,
  onScan,
  onRetry,
  onClearFilters,
  onColumnsChange,
}: GameGridProps) {
  const gridRef = useRef<HTMLDivElement>(null);

  // 条目太多时，切视图改用"整块淡入"，不做逐项位置动画
  const animateLayout = games.length <= LAYOUT_ANIM_LIMIT;

  const prevView = useRef(view);
  const [flashing, setFlashing] = useState(false);

  // 分页渲染：切视图 / 换筛选时回到第一页，避免一次性挂载全部卡片
  const [limit, setLimit] = useState(PAGE_SIZE);

  // 用 callback ref 挂 IntersectionObserver：
  // 网格被 AnimatePresence(mode="wait") 延迟挂载，useEffect 时机拿不到节点，
  // 而依赖又不会再变，观察器就永远挂不上 —— callback ref 在节点出现的那一刻执行，最可靠。
  const ioRef = useRef<IntersectionObserver | null>(null);
  const totalRef = useRef(games.length);
  totalRef.current = games.length;

  const sentinelRef = useCallback((node: HTMLDivElement | null) => {
    ioRef.current?.disconnect();
    ioRef.current = null;
    if (!node) return;

    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setLimit((l) => Math.min(l + PAGE_SIZE, totalRef.current));
        }
      },
      { rootMargin: "400px" },
    );
    io.observe(node);
    ioRef.current = io;
  }, []);

  useEffect(() => {
    setLimit(PAGE_SIZE);
  }, [view, games]);

  useEffect(() => () => ioRef.current?.disconnect(), []);

  const shownGames = games.slice(0, limit);

  useEffect(() => {
    if (prevView.current === view) return;
    prevView.current = view;
    if (animateLayout) return; // 小列表有魔法移动，不需要额外淡入
    setFlashing(true);
    const timer = window.setTimeout(() => setFlashing(false), 40);
    return () => window.clearTimeout(timer);
  }, [view, animateLayout]);

  // 量列数：取第一行里 offsetTop 相同的元素个数（grid 自动填充，列数随窗口变化）
  useEffect(() => {
    const el = gridRef.current;
    if (!el) return;

    const measure = () => {
      if (view === "list") {
        onColumnsChange(1);
        return;
      }
      const children = Array.from(el.children) as HTMLElement[];
      if (children.length === 0) {
        onColumnsChange(1);
        return;
      }
      const top = children[0].offsetTop;
      let cols = 0;
      for (const child of children) {
        if (child.offsetTop === top) cols += 1;
        else break;
      }
      onColumnsChange(cols || 1);
    };

    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [view, games.length, state, onColumnsChange]);

  return (
    <AnimatePresence mode="wait">
      {state === "loading" && (
        <motion.div
          key="loading"
          {...panelMotion}
          className="grid gap-x-4 gap-y-5 [grid-template-columns:repeat(auto-fill,minmax(140px,1fr))]"
        >
          {Array.from({ length: 12 }).map((_, i) => (
            <motion.div
              key={i}
              className="space-y-2"
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: i * 0.03, duration: 0.3 }}
            >
              <Skeleton className="aspect-[2/3] w-full rounded-xl" />
              <Skeleton className="h-3 w-3/4 rounded" />
              <Skeleton className="h-2.5 w-1/2 rounded" />
            </motion.div>
          ))}
        </motion.div>
      )}

      {state === "empty" && (
        <motion.div key="empty" {...panelMotion}>
          <StatePanel
            icon={<FolderSearch className="size-6 text-muted-foreground" />}
            title="这个书架还是空的"
            desc="选择一个游戏根目录，MoeShelf 会自动识别主程序、引擎和封面。"
            action={
              <Button onClick={onScan} className="gap-1.5">
                <ScanLine className="size-4" />
                扫描文件夹
              </Button>
            }
          />
        </motion.div>
      )}

      {state === "error" && (
        <motion.div key="error" {...panelMotion}>
          <StatePanel
            icon={<AlertTriangle className="size-6 text-amber-600" />}
            title="扫描时出了点问题"
            desc="目录 E:\galgame 无法读取，可能是权限不足或磁盘未连接。"
            action={
              <Button variant="outline" onClick={onRetry} className="gap-1.5">
                <RotateCw className="size-4" />
                重试
              </Button>
            }
          />
        </motion.div>
      )}

      {state === "normal" && games.length === 0 && (
        <motion.div key="nomatch" {...panelMotion}>
          <StatePanel
            icon={<FolderSearch className="size-6 text-muted-foreground" />}
            title="没有匹配的游戏"
            desc="当前筛选条件下没有任何结果，试试放宽筛选或清空搜索词。"
            action={
              <Button variant="ghost" onClick={onClearFilters} className="gap-1.5">
                <RotateCw className="size-4" />
                清空筛选
              </Button>
            }
          />
        </motion.div>
      )}

      {state === "normal" && games.length > 0 && (
        <motion.div
          key="grid"
          ref={gridRef}
          /* 容器不做 layout 动画：它只是改变自身高度，
             如果给高度加缓动，子元素会在 220ms 里被反复推挤；
             尺寸瞬间切换 + 子元素只做位置平移，才是"顺滑摊平"。 */
          style={{
            // 大列表切视图时用整块淡入遮掉重排瞬间（只有 1 个节点的开销）
            opacity: flashing ? 0.45 : 1,
            transition: "opacity 180ms cubic-bezier(0.4, 0, 0.2, 1)",
          }}
          className={cn(
            view === "grid"
              ? "grid gap-x-4 gap-y-5 [grid-template-columns:repeat(auto-fill,minmax(140px,1fr))]"
              : "flex flex-col gap-1",
          )}
        >
          {view === "list" && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.2 }}
              className="mb-1 flex items-center gap-3 px-2.5 pb-1"
            >
              <span className="w-11 shrink-0" />
              <span className="flex-1 text-[10.5px] tracking-wide text-muted-foreground">
                标题
              </span>
              <span className="hidden w-16 shrink-0 text-[10.5px] tracking-wide text-muted-foreground md:block">
                引擎
              </span>
              <span className="hidden w-16 shrink-0 text-[10.5px] tracking-wide text-muted-foreground lg:block">
                平台
              </span>
              <span className="w-12 shrink-0 text-right text-[10.5px] tracking-wide text-muted-foreground">
                评分
              </span>
              <span className="w-10 shrink-0 text-right text-[10.5px] tracking-wide text-muted-foreground">
                年份
              </span>
            </motion.div>
          )}

          <AnimatePresence mode="popLayout">
            {shownGames.map((game, i) => (
              <GameItem
                key={game.id}
                game={game}
                index={i}
                isGrid={view === "grid"}
                selected={selectedIds.includes(game.id)}
                cursor={cursorId === game.id}
                multi={multi}
                animateLayout={animateLayout}
                animateEntrance={i < ENTRANCE_LIMIT}
                onSelect={() => onSelect(game.id)}
                onOpen={() => onOpen(game.id)}
              />
            ))}
          </AnimatePresence>

          {/* 滚动到底部时继续追加（分页渲染的哨兵） */}
          {limit < games.length && (
            <div
              ref={sentinelRef}
              className={cn(
                "flex items-center justify-center py-6 text-[11px] text-muted-foreground",
                view === "grid" && "col-span-full",
              )}
            >
              已显示 {shownGames.length} / {games.length}，继续滚动加载…
            </div>
          )}
        </motion.div>
      )}
    </AnimatePresence>
  );
}
