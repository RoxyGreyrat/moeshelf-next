import { memo } from "react";
import { CircleCheck, Star } from "lucide-react";
import { motion } from "motion/react";
import type { Game } from "@/types/game";
import { engineLabel } from "@/types/game";
import { springHover } from "@/lib/motion";
import { CoverArt } from "./CoverArt";

interface GameCardProps {
  game: Game;
  /** 由父级 GameItem 的 hover 状态驱动，不再用 CSS group-hover */
  hovered: boolean;
  /** 多选态下的勾选圈状态 */
  multi: boolean;
  selected: boolean;
}

/**
 * 竖版封面卡片（纯视觉层，不含任何交互与选中框）
 * 交互全部在 GameItem 上，这样网格/列表切换时外框可以继续保持不动。
 *
 * 用 memo 包起来：列表视图下它是隐藏的，切视图时只要 props 没变就不重建 ——
 * 这是 212 部切换从 ~300ms 降到"几乎立刻能开始动画"的关键。
 */
export const GameCard = memo(function GameCard({
  game,
  hovered,
  multi,
  selected,
}: GameCardProps) {
  const year = game.released.slice(0, 4);

  return (
    <div className="relative aspect-[2/3] w-full overflow-hidden rounded-xl">
      {/* 封面：hover 时轻微推近，弹簧收尾 */}
      <motion.div
        className="absolute inset-0"
        animate={{ scale: hovered ? 1.045 : 1 }}
        transition={springHover}
        style={{ willChange: "transform" }}
      >
        <CoverArt game={game} />
      </motion.div>

      {/* 常驻底部渐变：不 hover 也能读标题 */}
      <div className="pointer-events-none absolute inset-x-0 bottom-0 h-3/5 bg-gradient-to-t from-black/80 via-black/30 to-transparent" />

      <motion.div
        className="absolute inset-x-0 bottom-0 p-3"
        animate={{ y: hovered ? -2 : 0 }}
        transition={springHover}
      >
        <p className="line-clamp-1 text-[13px] font-semibold leading-snug text-white">
          {game.titleCn}
        </p>
        <p className="mt-0.5 line-clamp-1 text-[11px] text-white/70">
          {game.developer} · {year}
        </p>

        {/* 元信息：hover 时浮出 */}
        <motion.div
          className="mt-1 flex items-center gap-1.5"
          initial={false}
          animate={{
            opacity: hovered ? 1 : 0,
            y: hovered ? 0 : 6,
          }}
          transition={springHover}
        >
          <Star className="size-3 fill-amber-300 text-amber-300" />
          <span className="text-[11px] tabular-nums text-white/90">
            {game.rating.toFixed(1)}
          </span>
          <span className="ml-auto rounded bg-white/15 px-1.5 py-0.5 text-[10px] font-medium text-white/90 backdrop-blur-sm">
            {engineLabel(game.engine)}
          </span>
        </motion.div>
      </motion.div>

      {game.completed && (
        <span className="absolute left-2 top-2 inline-flex items-center gap-1 rounded-full bg-black/35 px-1.5 py-0.5 text-[10px] text-white/90 backdrop-blur-sm">
          <CircleCheck className="size-3" />
          通关
        </span>
      )}

      {multi && (
        <motion.span
          className="absolute right-2 top-2 flex size-5 items-center justify-center rounded-full border"
          initial={false}
          animate={{
            backgroundColor: selected ? "var(--primary)" : "rgba(0,0,0,0.25)",
            borderColor: selected ? "var(--primary)" : "rgba(255,255,255,0.6)",
            scale: selected ? 1.06 : 1,
          }}
          transition={springHover}
        >
          <motion.span
            animate={{ opacity: selected ? 1 : 0, scale: selected ? 1 : 0.6 }}
            transition={springHover}
            className="text-primary-foreground"
          >
            <CircleCheck className="size-3.5" strokeWidth={3} />
          </motion.span>
        </motion.span>
      )}
    </div>
  );
});
