import { memo, useState } from "react";
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
 * 网格/列表的显隐由**纯 CSS** 决定（容器上的 `data-view`），
 * 所以这里根本不接收 `isGrid`：切视图时 212 个条目一个都不重建。
 *
 * 性能开关：
 *   - `animateLayout=false` → 完全不参与 layout 投影（211 条时每帧算 211 个包围盒会卡）
 *   - `animateEntrance=false` → 不挂 initial/animate，直接静态出现
 */
function _GameItem({
  game,
  index,
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
      whileTap={{ scale: 0.98, transition: springTap }}
      onHoverStart={() => setHovered(true)}
      onHoverEnd={() => setHovered(false)}
      onFocus={() => setHovered(true)}
      onBlur={() => setHovered(false)}
      onClick={onSelect}
      onDoubleClick={onOpen}
      className={cn(
        "game-item relative block w-full text-left outline-none",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
      )}
      style={{ willChange: "transform", transform: "translateZ(0)" }}
    >
      {/* 视觉层：卡片和行都常驻，显隐由容器上的 data-view 用 CSS 控制。
          切视图时 212 个条目一个都不重建 —— 动画才能在点下去的瞬间开始。 */}
      <div className="game-layer-grid" aria-hidden={false}>
        <GameCard
          game={game}
          hovered={hovered}
          multi={multi}
          selected={selected}
        />
      </div>
      <div className="game-layer-list" aria-hidden>
        <GameRow
          game={game}
          hovered={hovered}
          selected={selected}
          cursor={cursor}
          multi={multi}
        />
      </div>

      {/* 选中外框：从 0.95 弹开。圆角跟着视图走（CSS .game-ring） */}
      <motion.span
        aria-hidden="true"
        className="game-ring pointer-events-none absolute -inset-[3px] border-2 border-primary"
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
        className="game-ring pointer-events-none absolute -inset-[3px] border-2 border-primary/40"
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

/**
 * 自定义比较：忽略回调函数（每次渲染都是新箭头函数）。
 * 回调捕获的是 game.id，game 引用相同就不会出错。
 */
export const GameItem = memo(_GameItem, (a, b) => {
  return (
    a.game === b.game &&
    a.selected === b.selected &&
    a.cursor === b.cursor &&
    a.multi === b.multi &&
    a.animateLayout === b.animateLayout &&
    a.animateEntrance === b.animateEntrance &&
    a.index === b.index
  );
});
