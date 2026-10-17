import { useState } from "react";
import { motion, useReducedMotion } from "motion/react";
import type { Game } from "@/types/game";
import {
  ENTER_OFFSET,
  STAGGER,
  layoutTransition,
  springEnter,
  springRing,
  springTap,
} from "@/lib/motion";
import { cn } from "@/lib/utils";
import { GameCard } from "./GameCard";
import { GameRow } from "./GameRow";

interface GameItemProps {
  game: Game;
  index: number;
  isGrid: boolean;
  selected: boolean;
  cursor: boolean;
  multi: boolean;
  /** 是否启用 layout 位置动画。条目过多时关掉，否则逐帧投影会把主线程压死 */
  animateLayout: boolean;
  /** 是否播放入场动画。只为前 N 条开启，剩下的直接静态渲染 */
  animateEntrance: boolean;
  onSelect: () => void;
  onOpen: () => void;
}

/**
 * 卡片/行 的统一容器。
 *
 * 为什么网格和列表共用一个组件：
 *   切视图时如果换组件，DOM 会被销毁重建，只能"闪切"；
 *   共用同一个 motion.button + layout，切视图时它原地变形，才有"魔法移动"。
 *
 * 性能开关：
 *   - `animateLayout=false` → 完全不参与 layout 投影（211 条时每帧算 211 个包围盒会卡）
 *   - `animateEntrance=false` → 不挂 initial/animate，直接静态出现
 */
export function GameItem({
  game,
  index,
  isGrid,
  selected,
  cursor,
  multi,
  animateLayout,
  animateEntrance,
  onSelect,
  onOpen,
}: GameItemProps) {
  const [hovered, setHovered] = useState(false);
  const reduceMotion = useReducedMotion();
  const still = !animateEntrance || reduceMotion;

  return (
    <motion.button
      type="button"
      data-card-id={game.id}
      aria-pressed={selected}
      aria-label={`${game.titleCn}，${game.developer}，${game.released.slice(0, 4)} 年`}
      layout={animateLayout ? "position" : undefined}
      transition={{ layout: layoutTransition }}
      initial={still ? false : { opacity: 0, y: ENTER_OFFSET }}
      animate={
        still
          ? undefined
          : {
              opacity: 1,
              y: 0,
              // 错峰放在这里：只在首次挂载时生效，不影响后续 layout 重排
              transition: {
                ...springEnter,
                delay: Math.min(index, 11) * STAGGER,
              },
            }
      }
      exit={{ opacity: 0, scale: 0.96, transition: { duration: 0.16 } }}
      whileHover={isGrid ? { scale: 1.04, y: -4 } : undefined}
      whileTap={{ scale: 0.98, transition: springTap }}
      onHoverStart={() => setHovered(true)}
      onHoverEnd={() => setHovered(false)}
      onFocus={() => setHovered(true)}
      onBlur={() => setHovered(false)}
      onClick={onSelect}
      onDoubleClick={onOpen}
      className={cn(
        "relative block w-full text-left outline-none",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
      )}
      style={{ willChange: "transform", transform: "translateZ(0)" }}
    >
      {/* 视觉层：直接切换，不交叉淡化（旧海报按新宽度维持 2:3 会把高度撑爆）。
          但给新内容一个极短的淡入，避免"尺寸瞬间切换"时出现硬切感。 */}
      <motion.div
        key={isGrid ? "grid" : "list"}
        initial={{ opacity: 0.4 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15, ease: "easeOut" }}
      >
        {isGrid ? (
          <GameCard
            game={game}
            hovered={hovered}
            multi={multi}
            selected={selected}
          />
        ) : (
          <GameRow
            game={game}
            hovered={hovered}
            selected={selected}
            cursor={cursor}
            multi={multi}
          />
        )}
      </motion.div>

      {/* 选中外框：从 0.95 弹开 */}
      <motion.span
        aria-hidden="true"
        className={cn(
          "pointer-events-none absolute -inset-[3px] border-2 border-primary",
          isGrid ? "rounded-[15px]" : "rounded-[11px]",
        )}
        initial={false}
        animate={
          selected
            ? { opacity: 1, scale: 1 }
            : { opacity: 0, scale: 0.95 }
        }
        transition={springRing}
        style={{ willChange: "transform, opacity" }}
      />

      {/* 键盘光标（多选态下未选中那一格）：更淡的一圈 */}
      <motion.span
        aria-hidden="true"
        className={cn(
          "pointer-events-none absolute -inset-[3px] border-2 border-primary/40",
          isGrid ? "rounded-[15px]" : "rounded-[11px]",
        )}
        initial={false}
        animate={
          cursor && !selected
            ? { opacity: 1, scale: 1 }
            : { opacity: 0, scale: 0.97 }
        }
        transition={springRing}
      />
    </motion.button>
  );
}
