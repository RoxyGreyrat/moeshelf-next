import type { Transition } from "motion/react";

/**
 * 动效预设（全项目唯一来源）
 * 每次调手感只改这里，不要在组件里散写弹簧参数。
 */

/** 卡片入场：略微软一点，避免"弹过头" */
export const springEnter: Transition = {
  type: "spring",
  stiffness: 300,
  damping: 24,
};

/** 悬停抬升：要"跟手"，刚度更高、阻尼更大，收得住 */
export const springHover: Transition = {
  type: "spring",
  stiffness: 400,
  damping: 25,
};

/** 按压：极快，几乎瞬时 */
export const springTap: Transition = {
  type: "spring",
  stiffness: 600,
  damping: 30,
};

/**
 * 布局重排（网格 ↔ 列表）
 *
 * 手感目标："顺滑摊平 + 一点点收尾回弹"，不是橡皮筋。
 *
 * 关键在**阻尼比**，不是数大小：
 *   ζ = damping / (2 · √stiffness) = 33 / (2 · √600) ≈ 0.67
 *   ζ≈0.67 的理论超调约 5%，28px 的位移对应 1~2px 的视觉回弹 —— 手能感觉到，眼睛不会晕。
 *   ζ 越接近 1 越不回弹（30/√250 就是 0.95，基本等于 tween）；ζ 低于 0.5 开始明显晃。
 *
 * 刚度决定"多快收住"：260 太肉（收尾 450ms），600 大约 290ms 落定，同时保持同一条超调曲线。
 * 所以调手感时**优先动 stiffness，damping 按 2·ζ·√stiffness 等比跟**，否则幅度会跟着变。
 *
 * 同时尺寸变化由 layout="position" 直接跳过（见 GameItem），
 * 所以这里只需要关心"位置"这一条曲线，弹簧不会把卡片拉变形。
 */
export const layoutTransition: Transition = {
  type: "spring",
  stiffness: 600,
  damping: 33,
};

/** 选中外框：从 0.95 弹到 1，弹性明显但不夸张 */
export const springRing: Transition = {
  type: "spring",
  stiffness: 520,
  damping: 28,
};

/** 面板 / 浮层展开收起 */
export const springPanel: Transition = {
  type: "spring",
  stiffness: 420,
  damping: 32,
};

/** 卡片入场错峰间隔（秒）—— 40~50ms 之间 */
export const STAGGER = 0.045;

/** 入场位移距离：从下方 20px 浮起 */
export const ENTER_OFFSET = 20;
