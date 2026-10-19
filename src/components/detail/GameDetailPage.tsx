import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  FolderOpen,
  Pencil,
  Play,
  Star,
  Users,
} from "lucide-react";

import { getGameDetail } from "@/lib/api";
import type { GameDetail } from "@/lib/api";
import type { Character, Game } from "@/types/game";
import { engineLabel, PLATFORM_LABEL } from "@/types/game";
import { Button } from "@/components/ui/button";
import { coverSrc } from "@/lib/cover";
import { formatBytes, formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";

interface GameDetailPageProps {
  /** 要展示的游戏 id（就是 path_hash） */
  gameId: string;
  onClose: () => void;
  onPlay: () => void;
  /** 跳回书架并进入编辑态 */
  onEdit: () => void;
  onOpenFolder: () => void;
}

/** 角色头像加载失败时的兜底：从名字里取一个字，配一个按名字固定的底色 */
function CharacterFallback({ name, className }: { name: string; className?: string }) {
  let h = 5381;
  for (let i = 0; i < name.length; i += 1) h = ((h << 5) + h + name.charCodeAt(i)) >>> 0;
  const hue = h % 360;

  return (
    <div
      className={cn(
        "flex h-full w-full items-center justify-center",
        className,
      )}
      style={{
        background: `linear-gradient(150deg, hsl(${hue} 26% 88%), hsl(${(hue + 40) % 360} 22% 78%))`,
      }}
      aria-hidden="true"
    >
      <span className="select-none font-serif text-[34px] leading-none text-white/90">
        {name.slice(0, 1)}
      </span>
    </div>
  );
}

function CharacterCard({ c }: { c: Character }) {
  const src = coverSrc(c.imageUrl);
  const [broken, setBroken] = useState(false);

  return (
    <li className="min-w-0">
      <div className="relative aspect-[3/4] overflow-hidden rounded-xl bg-muted ring-1 ring-black/[0.06]">
        {src && !broken ? (
          <img
            src={src}
            alt={c.name}
            loading="lazy"
            decoding="async"
            onError={() => setBroken(true)}
            className="h-full w-full object-cover object-top"
          />
        ) : (
          <CharacterFallback name={c.name} />
        )}

        {c.role && (
          <span className="absolute left-1.5 top-1.5 rounded-full bg-black/45 px-1.5 py-[1px] text-[9.5px] font-medium text-white backdrop-blur-[2px]">
            {c.role}
          </span>
        )}
      </div>

      <p
        className="mt-1.5 truncate text-[12px] font-medium text-foreground"
        title={c.aliases.length > 0 ? `${c.name}（${c.aliases.join(" / ")}）` : c.name}
      >
        {c.name}
      </p>
      <p
        className={cn(
          "mt-0.5 truncate text-[10.5px]",
          c.cv ? "text-muted-foreground" : "text-muted-foreground/50",
        )}
        title={c.cv ? `${c.cv}${c.cvSource ? `（${c.cvSource}）` : ""}` : "旧库没有收录这位角色的声优"}
      >
        {c.cv ? `CV ${c.cv}` : "声优未收录"}
      </p>
    </li>
  );
}

/** 一行键值，用于右侧的文件信息 */
function MetaRow({ label, value, title }: { label: string; value: string; title?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-6 border-b border-border py-2 last:border-b-0">
      <span className="shrink-0 text-[11.5px] text-muted-foreground">{label}</span>
      <span
        className="min-w-0 truncate text-right text-[12px] text-foreground"
        title={title ?? value}
      >
        {value}
      </span>
    </div>
  );
}

/**
 * 全屏详情页。
 *
 * 走整页而不是弹窗/抽屉：角色网格一屏放得下 12~20 个人，弹窗会被高度卡住。
 * 数据从 `get_game_detail` 一次拿全（游戏 + 角色），不做分批加载 ——
 * 分两次拉会出现"标题先出来、角色区晚半秒"的跳变。
 */
export function GameDetailPage({
  gameId,
  onClose,
  onPlay,
  onEdit,
  onOpenFolder,
}: GameDetailPageProps) {
  const [detail, setDetail] = useState<GameDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** 长简介默认折 8 行，否则角色区会被顶到两屏之外 */
  const [descOpen, setDescOpen] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let alive = true;
    setDetail(null);
    setError(null);
    setDescOpen(false);
    scrollRef.current?.scrollTo({ top: 0 });

    getGameDetail(gameId)
      .then((d) => {
        if (alive) setDetail(d);
      })
      .catch((e) => {
        console.error("[get_game_detail]", e);
        if (alive) setError(String(e));
      });

    return () => {
      alive = false;
    };
  }, [gameId]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const game: Game | null = detail?.game ?? null;
  const characters = detail?.characters ?? [];
  const withCv = characters.filter((c) => c.cv).length;
  const poster = coverSrc(game?.coverUrl);
  const description = game?.cnDescription?.trim() || game?.description || "";

  return (
    <div className="flex h-full flex-col overflow-hidden bg-background text-foreground">
      {/* 顶栏：返回 + 当前标题 + 主操作 */}
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border px-5">
        <Button
          variant="ghost"
          size="sm"
          onClick={onClose}
          className="h-8 gap-1.5 px-2 text-[12px] text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="size-4" />
          返回书架
        </Button>
        <span className="h-4 w-px bg-border" />
        <p className="min-w-0 flex-1 truncate text-[12px] text-muted-foreground">
          {game ? game.titleCn || game.title : "读取中…"}
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={onEdit}
          disabled={!game}
          className="h-8 gap-1.5"
        >
          <Pencil className="size-3.5" />
          编辑信息
        </Button>
        <Button size="sm" onClick={onPlay} disabled={!game} className="h-8 gap-1.5">
          <Play className="size-3.5" />
          启动游戏
        </Button>
      </header>

      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        {error ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
            <span className="flex size-11 items-center justify-center rounded-full bg-amber-100 text-amber-600">
              <AlertTriangle className="size-5" />
            </span>
            <p className="text-[13px] font-medium">打不开这部游戏的详情</p>
            <p className="max-w-[420px] text-[11.5px] leading-relaxed text-muted-foreground">
              {error}
            </p>
            <Button variant="outline" size="sm" onClick={onClose} className="mt-1">
              返回书架
            </Button>
          </div>
        ) : !game ? (
          <>
            {/* 骨架：结构与真实内容一致，避免加载完成后大幅跳动 */}
            <div className="relative border-b border-border px-8 pb-7 pt-10">
              <div className="mx-auto flex w-full max-w-[1040px] items-end gap-6">
                <div className="aspect-[2/3] w-[150px] shrink-0 animate-pulse rounded-xl bg-muted" />
                <div className="flex-1 space-y-3 pb-2">
                  <div className="h-6 w-2/3 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-1/3 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-1/2 animate-pulse rounded bg-muted" />
                </div>
              </div>
            </div>
            <div className="mx-auto grid w-full max-w-[1040px] grid-cols-6 gap-4 px-8 py-7">
              {Array.from({ length: 6 }).map((_, i) => (
                <div key={i} className="aspect-[3/4] animate-pulse rounded-xl bg-muted" />
              ))}
            </div>
          </>
        ) : (
          <>
            {/* ---------------------------------------------------------- 头部 */}
            <div className="relative overflow-hidden border-b border-border">
              <div className="pointer-events-none absolute inset-0" aria-hidden="true">
                {poster ? (
                  <img
                    src={poster}
                    alt=""
                    className="h-full w-full scale-110 object-cover opacity-40 blur-2xl"
                  />
                ) : (
                  <div className="h-full w-full bg-gradient-to-br from-primary/[0.10] to-transparent" />
                )}
                <div className="absolute inset-0 bg-gradient-to-b from-background/50 via-background/75 to-background" />
              </div>

              <div className="relative mx-auto flex w-full max-w-[1040px] items-end gap-6 px-8 pb-7 pt-9">
                {/* 竖版封面 */}
                <div className="aspect-[2/3] w-[150px] shrink-0 overflow-hidden rounded-xl shadow-[0_18px_40px_-18px_rgba(24,24,27,0.5)] ring-1 ring-black/10">
                  {poster ? (
                    <img
                      src={poster}
                      alt={game.titleCn || game.title}
                      className="h-full w-full object-cover"
                    />
                  ) : (
                    <div className="flex h-full w-full items-center justify-center bg-gradient-to-br from-primary/25 to-primary/5">
                      <span className="select-none font-serif text-[56px] leading-none text-white/80">
                        {(game.titleCn || game.title).slice(0, 1)}
                      </span>
                    </div>
                  )}
                </div>

                <div className="min-w-0 flex-1 pb-1">
                  <div className="flex items-start gap-3">
                    <div className="min-w-0 flex-1">
                      <h1 className="truncate text-[24px] font-semibold leading-tight tracking-tight">
                        {game.titleCn || game.title}
                      </h1>
                      {game.originalTitle && game.originalTitle !== game.titleCn && (
                        <p className="mt-1 truncate text-[12.5px] text-muted-foreground">
                          {game.originalTitle}
                        </p>
                      )}
                      {game.title && game.title !== game.originalTitle && (
                        <p className="mt-0.5 truncate text-[11.5px] text-muted-foreground/80">
                          {game.title}
                        </p>
                      )}
                    </div>

                    {game.rating > 0 && (
                      <div className="flex shrink-0 items-center gap-1 rounded-lg bg-card px-2.5 py-1.5 shadow-[0_2px_10px_-6px_rgba(24,24,27,0.5)] ring-1 ring-border">
                        <Star className="size-3.5 fill-amber-400 text-amber-400" />
                        <span className="text-[13px] font-semibold tabular-nums">
                          {game.rating.toFixed(1)}
                        </span>
                        <span className="text-[10px] text-muted-foreground">
                          {game.ratingSource || ""}
                        </span>
                      </div>
                    )}
                  </div>

                  {/* 元信息：会社 · 发售 · 引擎 · 平台 */}
                  <div className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11.5px] text-muted-foreground">
                    {[
                      game.developer,
                      game.released,
                      engineLabel(game.engine),
                      PLATFORM_LABEL[game.platform],
                      game.completed ? "已通关" : "",
                    ]
                      .filter(Boolean)
                      .map((item, i) => (
                        <span key={`${item}-${i}`} className="flex items-center gap-2">
                          {i > 0 && <span className="text-border">·</span>}
                          {item}
                        </span>
                      ))}
                  </div>

                  {game.tags.length > 0 && (
                    <div className="mt-3 flex flex-wrap gap-1">
                      {game.tags.slice(0, 12).map((t) => (
                        <span
                          key={t}
                          className="rounded-full border border-border bg-card/70 px-2 py-[1px] text-[10.5px] text-muted-foreground"
                        >
                          {t}
                        </span>
                      ))}
                      {game.tags.length > 12 && (
                        <span className="px-1 text-[10.5px] text-muted-foreground/70">
                          +{game.tags.length - 12}
                        </span>
                      )}
                    </div>
                  )}
                </div>
              </div>
            </div>

            {/* ---------------------------------------------------------- 主体 */}
            <div className="mx-auto w-full max-w-[1040px] px-8 py-7">
              <div className="grid grid-cols-1 gap-8 lg:grid-cols-[minmax(0,1fr)_300px]">
                {/* 左：简介 + 角色 */}
                <div className="min-w-0 space-y-8">
                  <section>
                    <h2 className="text-[12.5px] font-medium text-foreground">简介</h2>
                    <p
                      className={cn(
                        "selectable mt-2 whitespace-pre-line text-[12px] leading-[1.85] text-muted-foreground",
                        // 长简介折 8 行：真实数据里 700~1500 字的简介很常见，
                        // 全展开会把角色区推到两三屏之外
                        !descOpen && description.length > 260 && "line-clamp-[8]",
                      )}
                    >
                      {description || "这部游戏还没有简介（旧库刮削时没抓到，或本身就是没收录的条目）。"}
                    </p>
                    {description.length > 260 && (
                      <button
                        type="button"
                        onClick={() => setDescOpen((v) => !v)}
                        className="mt-1.5 text-[11px] text-primary transition-opacity hover:opacity-75"
                      >
                        {descOpen ? "收起" : "展开全部"}
                      </button>
                    )}
                  </section>

                  <section>
                    <div className="flex items-baseline justify-between gap-3">
                      <h2 className="flex items-center gap-1.5 text-[12.5px] font-medium text-foreground">
                        <Users className="size-3.5 text-muted-foreground" />
                        角色
                      </h2>
                      <p className="text-[11px] tabular-nums text-muted-foreground">
                        {characters.length > 0
                          ? `共 ${formatCount(characters.length)} 位${
                              withCv > 0 ? ` · ${withCv} 位有配音` : ""
                            }`
                          : ""}
                      </p>
                    </div>

                    {characters.length === 0 ? (
                      <p className="mt-2 rounded-xl border border-dashed border-border px-4 py-8 text-center text-[11.5px] leading-relaxed text-muted-foreground">
                        这部还没有角色数据。
                        <br />
                        旧库刮削时只抓了 196 部，其余条目要等 D 阶段接上刮削后才有。
                      </p>
                    ) : (
                      <ul className="mt-3 grid grid-cols-[repeat(auto-fill,minmax(118px,1fr))] gap-x-3 gap-y-4">
                        {characters.map((c) => (
                          <CharacterCard key={`${c.name}-${c.sortOrder}`} c={c} />
                        ))}
                      </ul>
                    )}
                  </section>
                </div>

                {/* 右：文件信息 */}
                <aside className="min-w-0">
                  <h2 className="text-[12.5px] font-medium text-foreground">文件</h2>
                  <div className="mt-2 rounded-xl border border-border bg-card px-3.5 py-1">
                    <MetaRow
                      label="目录"
                      value={game.folderPath}
                      title={game.folderPath}
                    />
                    <MetaRow
                      label="主程序"
                      value={game.launchExe || "未识别"}
                      title={game.launchExe}
                    />
                    <MetaRow label="文件数" value={`${formatCount(game.fileCount)} 个`} />
                    {game.dirSize > 0 && (
                      <MetaRow label="体积" value={formatBytes(game.dirSize)} />
                    )}
                    <MetaRow label="入库" value={game.firstSeenAt.slice(0, 10)} />
                  </div>

                  <Button
                    variant="outline"
                    size="sm"
                    onClick={onOpenFolder}
                    className="mt-2 h-8 w-full gap-1.5"
                  >
                    <FolderOpen className="size-3.5" />
                    在资源管理器中打开
                  </Button>

                  {game.lockedFields.length > 0 && (
                    <p className="mt-3 text-[10.5px] leading-relaxed text-muted-foreground/70">
                      已锁定 {game.lockedFields.length} 个字段，重扫和刮削都不会覆盖。
                    </p>
                  )}
                </aside>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
