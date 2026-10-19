import { useEffect, useRef } from "react";
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
 *   改用"整块淡入 + 轻微上移"过渡：一次合成层动画，主线程几乎零成本。
 * - 只有前 36 条播入场动画，剩下的直接静态渲染，避免 211 个弹簧同时启动。
 */
const LAYOUT_ANIM_LIMIT = 120;
const ENTRANCE_LIMIT = 36;

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

  // 视图切换的动效交给浏览器的 View Transitions（见 App.handleViewChange + index.css），
  // 这里不再自己播容器动画：重排和截图同时做会互相打架，反而更卡。

  // 全部渲染，不再"滚到底再追加"。
  // 211 个节点对 WebView2 完全撑得住；分页会让滚动条长度一直变、"已显示 40/212"的提示
  // 也显得像半成品。重排成本由上面的整块动画遮住。
  const shownGames = games;

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
            desc="选择一个游戏根目录，MoeShelf 会识别主程序、引擎和目录体积。"
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
            title="书架没读出来"
            desc="数据库可能被别的进程占用，或者文件损坏了。重试一次通常能好；一直不行就去设置里看看数据目录还在不在。"
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
          data-grid-root="true"
          data-view={view}
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
        </motion.div>
      )}
    </AnimatePresence>
  );
}
