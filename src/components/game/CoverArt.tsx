import type { Game } from "@/types/game";
import { coverSrc } from "@/lib/cover";
import { cn } from "@/lib/utils";

/** 低饱和双色板：没有 coverColors 时按 id 哈希取一组，保证同一部游戏颜色稳定 */
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
 * 封面。
 *
 * ⚠️ 性能约束：库里 200+ 部游戏，这里**不能**用 SVG。
 * 早先每张卡一棵内联 SVG（渐变 + pattern 斜纹 + clipPath + 水印），
 * 切视图时 211 棵 SVG 树重建 + 重新栅格化，直接掉到 4 帧/900ms。
 * 现在全部换成 CSS 图层 + 一张 <img>，纯合成属性，成本极低。
 *
 * - 有 coverUrl：走 `cover://` 协议读旧缓存（Rust 那边补 Content-Type）
 * - 没有 / 加载失败：下面那层 CSS 渐变就是兜底，不留破图
 * - 水印只在没有真封面时才画，且只是一段文字
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
  const wide = variant === "wide";
  const imgSrc = coverSrc(game.coverUrl);
  const glyph = game.titleCn.slice(0, 1);

  return (
    <div className={cn("relative h-full w-full overflow-hidden", className)}>
      {/* 底色渐变 */}
      <div
        className="absolute inset-0"
        style={{ backgroundImage: `linear-gradient(135deg, ${c1}, ${c2})` }}
      />

      {/* 斜纹：重复线性渐变，代替 SVG pattern */}
      <div
        className="absolute inset-0 opacity-[0.09]"
        style={{
          backgroundImage:
            "repeating-linear-gradient(45deg, #ffffff 0 1px, transparent 1px 16px)",
        }}
      />

      {/* 右上高光 */}
      <div
        className="absolute inset-0"
        style={{
          backgroundImage:
            "radial-gradient(80% 80% at 70% 15%, rgba(255,255,255,0.40), transparent)",
        }}
      />

      {imgSrc ? (
        <img
          src={imgSrc}
          alt=""
          /* 不用 lazy：封面全部来自本地 `cover://` 缓存，读取是毫秒级。
             lazy 会让"切到列表视图 / 往下滚"时一张张蹦出来 ——
             那正是"列表一点一点加载"的观感来源。 */
          loading="eager"
          decoding="async"
          draggable={false}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : (
        !wide && (
          <span
            aria-hidden="true"
            className="pointer-events-none absolute inset-0 flex items-center justify-center text-white/[0.17] select-none"
            style={{
              fontFamily:
                '"Noto Serif SC", "Source Han Serif SC", "Songti SC", SimSun, serif',
              fontSize: "9rem",
              fontWeight: 700,
              lineHeight: 1,
            }}
          >
            {glyph}
          </span>
        )
      )}

      {/* 内描边 */}
      <div className="pointer-events-none absolute inset-[8px] border border-white/15" />

      {/* 底部渐暗，给下层标题做衬底 */}
      <div
        className="pointer-events-none absolute inset-0"
        style={{
          backgroundImage:
            "linear-gradient(to bottom, transparent 50%, rgba(0,0,0,0.34))",
        }}
      />
    </div>
  );
}
