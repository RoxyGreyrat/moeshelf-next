import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { ChevronRight, FileCog, Loader2, Play, Square, X } from "lucide-react";
import { toast } from "sonner";

import {
  getPlaytime,
  launchGame,
  listExeCandidates,
  onGameExited,
  stopGame,
  updateGame,
} from "@/lib/api";
import type { ExeCandidate, PlaytimeInfo } from "@/lib/api";
import type { Game } from "@/types/game";
import { Button } from "@/components/ui/button";
import { PathPickerDialog } from "@/components/common/PathPickerDialog";
import { springPanel } from "@/lib/motion";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";

interface LaunchControlProps {
  game: Game;
  /** 启动项被确认后，把更新过的行交回上层（刷新列表/详情） */
  onGameUpdated?: (g: Game) => void;
  /** 面板在按钮上方还是下方：侧栏用 top，详情页顶栏用 bottom */
  placement?: "top" | "bottom";
}

/** 0:12 / 1:02:03 */
function clock(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${m}:${String(sec).padStart(2, "0")}`;
}

/** 12 分钟 / 3.4 小时 */
function duration(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes <= 0) return "还没玩过";
  if (minutes < 60) return `${Math.round(minutes)} 分钟`;
  const h = minutes / 60;
  return h >= 10 ? `${h.toFixed(0)} 小时` : `${h.toFixed(1)} 小时`;
}

/**
 * 启动游戏 —— 按钮 + 弹出面板。
 *
 * 规则（产品要求）：
 * 1. **每个游戏首次启动前都要确认一次启动程序**（扫描器猜的主程序可能不对）；
 * 2. 确认过之后就记住（写入并锁定 `launchExe`），以后再点直接启动；
 * 3. 点击启动后，**在按钮上方弹出面板**显示运行状态与计时；
 * 4. 随时可以在面板里「更改启动项」。
 */
export function LaunchControl({ game, onGameUpdated, placement = "top" }: LaunchControlProps) {
  const [pt, setPt] = useState<PlaytimeInfo | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [view, setView] = useState<"launch" | "choose">("launch");
  const [candidates, setCandidates] = useState<ExeCandidate[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [elapsed, setElapsed] = useState(0);

  const wrapRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    try {
      setPt(await getPlaytime(game.id));
    } catch (err) {
      console.error("[get_playtime]", err);
    }
  }, [game.id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // 进程退出（无论是不是从这里启动的）都要刷新状态与时长
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void onGameExited((payload) => {
      if (payload.gameId === game.id) void refresh();
    }).then((fn) => {
      if (alive) unlisten = fn;
      else fn();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [game.id, refresh]);

  // 运行中每秒走一次表
  useEffect(() => {
    const base = pt?.runningSeconds ?? 0;
    setElapsed(base);
    if (!pt?.running) return;
    const startedAt = Date.now();
    const timer = window.setInterval(
      () => setElapsed(base + Math.floor((Date.now() - startedAt) / 1000)),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [pt?.running, pt?.runningSeconds]);

  // 点面板外面收起
  useEffect(() => {
    if (!panelOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setPanelOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPanelOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [panelOpen]);

  const running = !!pt?.running;
  /** 没确认过启动项 → 首次启动，必须先选 */
  const needsConfirm = !game.launchExe || !game.lockedFields.includes("launchExe");

  const openChooser = useCallback(async () => {
    setPanelOpen(true);
    setView("choose");
    setError(null);
    setLoading(true);
    try {
      setCandidates(await listExeCandidates(game.id));
    } catch (err) {
      console.error("[list_exe_candidates]", err);
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [game.id]);

  const start = useCallback(
    async (exe?: string) => {
      setBusy(exe ?? "__default");
      setError(null);
      try {
        if (exe) {
          // 确认启动项：写回并锁定，界面上会立刻出现小锁
          const updated = await updateGame(game.id, { launchExe: exe }, true);
          onGameUpdated?.(updated);
        }
        await launchGame(game.id, exe);
        setPanelOpen(true);
        setView("launch");
        await refresh();
      } catch (err) {
        console.error("[launch_game]", err);
        setError(String(err));
        // 启动失败最常见的原因是主程序不在了 / 选错了 → 直接把候选列表摆出来
        if (!exe) await openChooser();
      } finally {
        setBusy(null);
      }
    },
    [game.id, onGameUpdated, openChooser, refresh],
  );

  const onPrimary = () => {
    if (running) {
      setPanelOpen((v) => !v);
      return;
    }
    if (needsConfirm) {
      void openChooser();
      return;
    }
    void start();
  };

  const stop = async () => {
    try {
      await stopGame(game.id);
    } catch (err) {
      console.error("[stop_game]", err);
      toast.error("结束失败", { description: String(err) });
    }
  };

  return (
    <div
      ref={wrapRef}
      className={cn("relative", placement === "top" ? "w-full" : "inline-block")}
    >
      <AnimatePresence>
        {panelOpen && (
          <motion.div
            key="launch-panel"
            initial={{ opacity: 0, y: placement === "top" ? 10 : -10, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: placement === "top" ? 8 : -8, scale: 0.97 }}
            transition={springPanel}
            style={{
              transformOrigin: placement === "top" ? "bottom center" : "top center",
            }}
            className={cn(
              "absolute z-40 overflow-hidden rounded-xl border border-border bg-card p-3 shadow-[0_16px_40px_-16px_rgba(24,24,27,0.35)]",
              placement === "top"
                ? "bottom-full left-0 right-0 mb-2"
                : // 顶栏底下没有横向空间：面板比按钮宽，向右对齐，向左展开
                  "right-0 top-full mt-2 w-[340px]",
            )}
          >
            {view === "choose" ? (
              <div>
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <p className="text-[12.5px] font-medium text-foreground">
                      选择启动程序
                    </p>
                    <p className="mt-0.5 text-[11px] leading-relaxed text-muted-foreground">
                      第一次启动确认一次，以后直接启动；随时可以改。
                    </p>
                  </div>
                  <button
                    type="button"
                    aria-label="收起"
                    onClick={() => setPanelOpen(false)}
                    className="rounded p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  >
                    <X className="size-3.5" />
                  </button>
                </div>

                {error && (
                  <p className="mt-2 rounded-md bg-destructive/5 px-2 py-1.5 text-[11px] leading-relaxed text-destructive">
                    {error}
                  </p>
                )}

                <div className="mt-2 max-h-[196px] space-y-0.5 overflow-y-auto">
                  {loading ? (
                    <div className="flex h-16 items-center justify-center">
                      <Loader2 className="size-4 animate-spin text-muted-foreground" />
                    </div>
                  ) : candidates.length === 0 ? (
                    <p className="px-2 py-3 text-[11.5px] text-muted-foreground">
                      这个目录里没有找到 .exe，用下面的「手动选择」指定。
                    </p>
                  ) : (
                    candidates.map((c) => (
                      <button
                        key={c.path}
                        type="button"
                        disabled={!!busy}
                        onClick={() => void start(c.path)}
                        className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-muted/70 disabled:opacity-50"
                      >
                        <FileCog className="size-3.5 shrink-0 text-muted-foreground" />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[12px] text-foreground">
                            {c.name}
                          </span>
                          <span className="block truncate text-[10px] text-muted-foreground">
                            {formatBytes(c.size)} · {c.path}
                          </span>
                        </span>
                        {busy === c.path ? (
                          <Loader2 className="size-3.5 shrink-0 animate-spin text-primary" />
                        ) : (
                          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground/60" />
                        )}
                      </button>
                    ))
                  )}
                </div>

                <div className="mt-2 flex items-center justify-between border-t border-border pt-2">
                  <button
                    type="button"
                    onClick={() => setPickerOpen(true)}
                    className="text-[11.5px] text-primary transition-opacity hover:opacity-75"
                  >
                    手动选择其他程序…
                  </button>
                  <span className="text-[10.5px] text-muted-foreground">
                    {(pt?.sessions ?? 0) > 0
                      ? `已启动 ${pt?.sessions} 次`
                      : "还没有启动过"}
                  </span>
                </div>
              </div>
            ) : running ? (
              <div>
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
                      <span className="relative flex size-1.5">
                        <span className="absolute inline-flex size-full animate-ping rounded-full bg-primary/70" />
                        <span className="relative inline-flex size-1.5 rounded-full bg-primary" />
                      </span>
                      正在游玩
                    </p>
                    <p className="mt-1 font-mono text-[24px] leading-none tabular-nums text-foreground">
                      {clock(elapsed)}
                    </p>
                  </div>
                  <button
                    type="button"
                    aria-label="收起"
                    onClick={() => setPanelOpen(false)}
                    className="rounded p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  >
                    <X className="size-3.5" />
                  </button>
                </div>

                <div className="mt-3 flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 flex-1 gap-1 text-[11.5px]"
                    disabled={!!busy}
                    onClick={() => void openChooser()}
                  >
                    更改启动项
                  </Button>
                  <Button
                    variant="destructive"
                    size="sm"
                    className="h-8 flex-1 gap-1 text-[11.5px]"
                    onClick={() => void stop()}
                  >
                    <Square className="size-3" />
                    结束游戏
                  </Button>
                </div>

                <p className="mt-2 text-[10.5px] text-muted-foreground">
                  共启动 {pt?.sessions ?? 0} 次 · 累计 {duration(pt?.minutes ?? 0)}
                </p>
              </div>
            ) : (
              /* 刚启动过、进程已经退出：给一个收尾状态，而不是突然变回"选程序" */
              <div>
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <p className="text-[12.5px] font-medium text-foreground">游戏已退出</p>
                    <p className="mt-0.5 text-[11px] text-muted-foreground">
                      共启动 {pt?.sessions ?? 0} 次 · 累计 {duration(pt?.minutes ?? 0)}
                    </p>
                  </div>
                  <button
                    type="button"
                    aria-label="收起"
                    onClick={() => setPanelOpen(false)}
                    className="rounded p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  >
                    <X className="size-3.5" />
                  </button>
                </div>
                <div className="mt-3 flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 flex-1 text-[11.5px]"
                    onClick={() => void openChooser()}
                  >
                    更改启动项
                  </Button>
                  <Button
                    size="sm"
                    className="h-8 flex-1 gap-1 text-[11.5px]"
                    disabled={!!busy}
                    onClick={() => void start()}
                  >
                    <Play className="size-3" />
                    再启动一次
                  </Button>
                </div>
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      <Button
        onClick={onPrimary}
        disabled={busy === "__default"}
        className={cn(
          "h-9 gap-1.5",
          placement === "top" ? "w-full" : "px-3.5 text-[12.5px]",
        )}
      >
        {busy === "__default" ? (
          <Loader2 className="size-4 animate-spin" />
        ) : running ? (
          <span className="relative flex size-2">
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-primary-foreground/60" />
            <span className="relative inline-flex size-2 rounded-full bg-primary-foreground" />
          </span>
        ) : (
          <Play className="size-4" />
        )}
        {busy === "__default"
          ? "启动中…"
          : running
            ? `运行中 ${clock(elapsed)}`
            : "启动游戏"}
      </Button>

      <PathPickerDialog
        open={pickerOpen}
        mode="exe"
        title="选择启动程序"
        startAt={game.folderPath}
        onOpenChange={setPickerOpen}
        onPick={(path) => {
          setPickerOpen(false);
          void start(path);
        }}
      />
    </div>
  );
}
