import type { Game } from "@/types/game";
import { cn } from "@/lib/utils";

/** 低饱和双色板：真库记录没有 coverColors 时，按 id 哈希取一组，保证同一部游戏颜色稳定 */
const FALLBACK_PALETTES: [string, string][] = [
  ["#5B5BD6", "#9B8AFB"],
  ["#2E7DA8", "#7FC4D8"],
  ["#7C3A5A", "#C08497"],
  ["#B4685A", "#E8B4A0"],
  ["#4A5568", "#94A3B8"],
  ["#4B3F5E", "#8B7BA0"],
  ["#2C7A7B", "#7EC8C3"],
  ["#3F4A6B", "#8CA0C4"],
  ["#C2708F", "#F0C0CF"],
  ["#5A4A7A", "#A18BC7"],
  ["#6B7F5E", "#B8CBA0"],
  ["#8A5A3B", "#D9B08C"],
];

function paletteFor(game: Game): [string, string] {
  if (game.coverColors) return game.coverColors;
  let hash = 0;
  for (const ch of game.id) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return FALLBACK_PALETTES[hash % FALLBACK_PALETTES.length];
}

/**
 * 假封面：低饱和双色渐变 + 细斜纹 + 宋体大字水印 + 内描边。
 * 真库接入后这里换成 <img src={coverUrl} />，外层比例与遮罩保持不变。
 *
 * variant="poster"（默认，2:3 竖版）：带宋体水印
 * variant="wide"（详情面板顶部 16:9 横图）：只留渐变与斜纹，
 *   因为把竖版水印裁进横图会出现一个巨大的、被切掉的大字，很脏。
 */
export function CoverArt({
  game,
  className,
  variant = "poster",
}: {
  game: Game;
  className?: string;
  variant?: "poster" | "wide";
}) {
  const [c1, c2] = paletteFor(game);
  const gid = `g-${game.id}`;
  const rid = `r-${game.id}`;
  const pid = `p-${game.id}`;
  const vid = `v-${game.id}`;
  const glyph = game.titleCn.slice(0, 1);
  const wide = variant === "wide";
  const h = wide ? 169 : 450;

  return (
    <svg
      viewBox={`0 0 300 ${h}`}
      preserveAspectRatio="xMidYMid slice"
      className={cn("h-full w-full", className)}
      aria-hidden="true"
      role="presentation"
    >
      <defs>
        <linearGradient id={gid} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor={c1} />
          <stop offset="100%" stopColor={c2} />
        </linearGradient>

        <radialGradient id={rid} cx="0.7" cy="0.15" r="0.8">
          <stop offset="0%" stopColor="#ffffff" stopOpacity="0.4" />
          <stop offset="100%" stopColor="#ffffff" stopOpacity="0" />
        </radialGradient>

        {/* 45° 细斜纹，模拟印刷网点，避免纯色的廉价感 */}
        <pattern
          id={pid}
          width="16"
          height="16"
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <line
            x1="0"
            y1="0"
            x2="0"
            y2="16"
            stroke="#ffffff"
            strokeWidth="1"
            strokeOpacity="0.09"
          />
        </pattern>

        <linearGradient id={vid} x1="0" y1="0.5" x2="0" y2="1">
          <stop offset="0%" stopColor="#000000" stopOpacity="0" />
          <stop offset="100%" stopColor="#000000" stopOpacity="0.34" />
        </linearGradient>
      </defs>

      <rect width="300" height={h} fill={`url(#${gid})`} />
      <rect width="300" height={h} fill={`url(#${pid})`} />
      <rect width="300" height={h} fill={`url(#${rid})`} />

      {/* 宋体大字水印：只有低透明度时才好看，别提上去 */}
      {!wide && (
        <text
          x="150"
          y="268"
          textAnchor="middle"
          fontSize="176"
          fontWeight={700}
          fill="#ffffff"
          fillOpacity="0.17"
          style={{
            fontFamily:
              '"Noto Serif SC", "Source Han Serif SC", "Songti SC", SimSun, serif',
          }}
        >
          {glyph}
        </text>
      )}

      <rect
        x="8"
        y="8"
        width="284"
        height={h - 16}
        fill="none"
        stroke="#ffffff"
        strokeOpacity="0.16"
        strokeWidth="1"
      />
      <rect width="300" height={h} fill={`url(#${vid})`} />
    </svg>
  );
}
