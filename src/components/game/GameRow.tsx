import { CircleCheck, Star } from "lucide-react";
import { motion } from "motion/react";
import type { Game } from "@/types/game";
import { ENGINE_LABEL, PLATFORM_LABEL } from "@/types/game";
import { springHover } from "@/lib/motion";
import { CoverArt } from "./CoverArt";

interface GameRowProps {
  game: Game;
  hovered: boolean;
  selected: boolean;
  cursor: boolean;
  multi: boolean;
}

/**
 * 列表行（纯视觉层）。外层的 motion.button / layout 由 GameItem 统一负责，
 * 所以从网格切到列表时，同一个 DOM 节点会原地变形，而不是闪一下换组件。
 */
export function GameRow({ game, hovered, selected, cursor, multi }: GameRowProps) {
  return (
    <motion.div
      className="flex w-full items-center gap-3 rounded-lg border px-2.5 py-2"
      initial={false}
      animate={{
        backgroundColor: selected
          ? "color-mix(in oklab, var(--accent) 70%, transparent)"
          : hovered
            ? "color-mix(in oklab, var(--muted) 60%, transparent)"
            : "rgba(0,0,0,0)",
        borderColor: selected || cursor ? "color-mix(in oklab, var(--primary) 30%, transparent)" : "rgba(0,0,0,0)",
      }}
      transition={springHover}
    >
      <div className="h-16 w-11 shrink-0 overflow-hidden rounded-md ring-1 ring-black/10">
        <CoverArt game={game} />
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <p className="truncate text-[13px] font-medium text-foreground">
            {game.titleCn}
          </p>
          {game.completed && (
            <CircleCheck className="size-3.5 shrink-0 text-primary" />
          )}
          {multi && (
            <motion.span
              className="ml-1 size-2 rounded-full border"
              initial={false}
              animate={{
                backgroundColor: selected ? "var(--primary)" : "rgba(0,0,0,0)",
                borderColor: selected ? "var(--primary)" : "var(--border)",
              }}
            />
          )}
        </div>
        <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
          {game.title} · {game.developer}
        </p>
        <div className="mt-1 flex flex-wrap gap-1">
          {game.tags.slice(0, 3).map((t) => (
            <span
              key={t}
              className="rounded-full bg-muted px-1.5 py-px text-[10px] text-muted-foreground"
            >
              {t}
            </span>
          ))}
        </div>
      </div>

      <div className="hidden w-16 shrink-0 text-[11px] text-muted-foreground md:block">
        {ENGINE_LABEL[game.engine]}
      </div>
      <div className="hidden w-16 shrink-0 text-[11px] text-muted-foreground lg:block">
        {PLATFORM_LABEL[game.platform]}
      </div>
      <div className="flex w-12 shrink-0 items-center justify-end gap-1 text-[12px] tabular-nums text-foreground">
        <Star className="size-3 fill-amber-400 text-amber-400" />
        {game.rating.toFixed(1)}
      </div>
      <div className="w-10 shrink-0 text-right text-[11px] tabular-nums text-muted-foreground">
        {game.released.slice(0, 4)}
      </div>
    </motion.div>
  );
}
