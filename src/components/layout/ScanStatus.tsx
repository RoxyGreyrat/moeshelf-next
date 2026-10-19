import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { Loader2 } from "lucide-react";

import type { AutoMatchProgress, ScanProgress } from "@/lib/api";

/**
 * 扫描进度（嵌在书架顶部那行状态文案里，占据右侧"快捷键提示"的位置）。
 *
 * 为什么不做成浮层/进度条：扫描是后台任务，不该盖住封面、也不该让网格上下跳。
 * 这一行本来就常驻、不随滚动移动，把提示文案临时换成进度是最安静的做法。
 */
export function ScanStatus({ progress }: { progress: ScanProgress | null }) {
  const reduce = useReducedMotion();

  return (
    <AnimatePresence mode="wait">
      {progress && (
        <motion.div
          key="scan-status"
          role="status"
          aria-live="polite"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: reduce ? 0 : 0.15 }}
          className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground"
        >
          <Loader2 className="size-3.5 shrink-0 animate-spin text-primary" />
          <span className="shrink-0">
            正在扫描{" "}
            <span className="font-medium tabular-nums text-foreground">
              {progress.dirsScanned.toLocaleString("zh-CN")}
            </span>{" "}
            个文件夹 · 发现{" "}
            <span className="font-medium tabular-nums text-foreground">
              {progress.gamesFound}
            </span>{" "}
            部
          </span>
          <span className="max-w-[260px] truncate text-muted-foreground/60">
            {progress.current ? `· ${progress.current}` : ""}
          </span>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/**
 * 自动匹配资料的进度。
 *
 * 和扫描共用同一行状态位：扫描结束后紧接着跑匹配，用户看到的是同一块地方
 * 从"正在扫描"平滑换成"正在匹配资料 3/18"，不需要再学一个新控件。
 */
export function MatchStatus({ progress }: { progress: AutoMatchProgress | null }) {
  const reduce = useReducedMotion();

  return (
    <AnimatePresence mode="wait">
      {progress && (
        <motion.div
          key="match-status"
          role="status"
          aria-live="polite"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: reduce ? 0 : 0.15 }}
          className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground"
        >
          <Loader2 className="size-3.5 shrink-0 animate-spin text-primary" />
          <span className="shrink-0">
            正在匹配资料{" "}
            <span className="font-medium tabular-nums text-foreground">
              {Math.min(progress.done + 1, progress.total)}
            </span>
            <span className="text-muted-foreground/60">/{progress.total}</span>
          </span>
          <span className="max-w-[280px] truncate text-muted-foreground/60">
            {progress.title}
          </span>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
