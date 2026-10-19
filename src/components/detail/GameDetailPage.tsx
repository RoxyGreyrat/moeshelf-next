import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  FolderOpen,
  ImageIcon,
  Pencil,
  Play,
  Sparkles,
  Star,
  Users,
} from "lucide-react";
import { toast } from "sonner";

import { getGameDetail, setCoverFromFile } from "@/lib/api";
import type { GameDetail, ScrapeReport } from "@/lib/api";
import type { Character, Game } from "@/types/game";
import { engineLabel, PLATFORM_LABEL } from "@/types/game";
import { Button } from "@/components/ui/button";
import { coverSrc } from "@/lib/cover";
import { formatBytes, formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";
import { GameEditDialog } from "@/components/detail/GameEditDialog";
import { ScrapeDialog } from "@/components/detail/ScrapeDialog";
import { PathPickerDialog } from "@/components/common/PathPickerDialog";

interface GameDetailPageProps {
  /** 要展示的游戏 id（就是 path_hash） */
  gameId: string;
  onClose: () => void;
  onPlay: () => void;
  onOpenFolder: () => void;
}

/** 页面内容的最大宽度。窗口再宽也不无限拉长，但比原来的 1040 宽得多 */
const PAGE = "mx-auto w-full max-w-[1680px] px-10";

/** 角色头像加载失败时的兜底：从名字里取一个字，配一个按名字固定的底色 */
function CharacterFallback({ name }: { name: string }) {
  let h = 5381;
  for (let i = 0; i < name.length; i += 1) h = ((h << 5) + h + name.charCodeAt(i)) >>> 0;
  const hue = h % 360;

  return (
    <div
      className="flex h-full w-full items-center justify-center"
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
    <li className="group min-w-0">
      <div className="relative aspect-[3/4] overflow-hidden rounded-xl bg-muted ring-1 ring-black/[0.06]">
        {src && !broken ? (
          <img
            src={src}
            alt={c.name}
            loading="lazy"
            decoding="async"
            onError={() => setBroken(true)}
            className="h-full w-full object-cover object-top transition-transform duration-300 ease-out group-hover:scale-[1.04]"
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

/** 头部右侧的关键数据：把原来散在标题行里的东西集中起来，填掉宽窗口的空洞 */
function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0">
      <p className="text-[10.5px] tracking-[0.02em] text-muted-foreground">{label}</p>
      <p className="mt-0.5 truncate text-[12.5px] font-medium text-foreground">{children}</p>
    </div>
  );
}

function MetaRow({ label, value, title }: { label: string; value: string; title?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-5 border-b border-border py-2 last:border-b-0">
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

function SectionHeading({
  children,
  right,
}: {
  children: React.ReactNode;
  right?: React.ReactNode;
}) {
  return (
    <div className="mb-3 flex items-baseline justify-between gap-4 border-b border-border pb-2">
      <h2 className="text-[12.5px] font-medium tracking-wide text-foreground">{children}</h2>
      {right}
    </div>
  );
}

/**
 * 全屏详情页。
 *
 * 走整页而不是弹窗/抽屉：角色网格一屏要放几十个人，弹窗会被高度卡住。
 * 数据从 `get_game_detail` 一次拿全（游戏 + 角色），不做分批加载 ——
 * 分两次拉会出现"标题先出来、角色区晚半秒"的跳变。
 *
 * 布局是流式的：内容最宽 1680，靠 `auto-fill` 的网格吃掉多出来的宽度。
 * 旧版固定 1040 居中，窗口一大两边就是两条空白带。
 */
export function GameDetailPage({
  gameId,
  onClose,
  onPlay,
  onOpenFolder,
}: GameDetailPageProps) {
  const [detail, setDetail] = useState<GameDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** 长简介默认折 8 行，否则角色区会被顶到两屏之外 */
  const [descOpen, setDescOpen] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  /** 编辑 / 刮削 / 换封面 —— 都在这个页面里完成，不再跳回侧边栏 */
  const [editOpen, setEditOpen] = useState(false);
  const [scrapeOpen, setScrapeOpen] = useState(false);
  const [coverPickerOpen, setCoverPickerOpen] = useState(false);
  const [coverBusy, setCoverBusy] = useState(false);
  /** 变了就重新拉一次详情（保存 / 刮削 / 换封面之后） */
  const [refreshKey, setRefreshKey] = useState(0);
  const refresh = () => setRefreshKey((k) => k + 1);

  const pickCover = async (path: string) => {
    setCoverBusy(true);
    try {
      await setCoverFromFile(gameId, path);
      refresh();
      toast.success("封面已更新");
    } catch (err) {
      console.error("[set_cover_from_file]", err);
      toast.error("换封面失败", { description: String(err) });
    } finally {
      setCoverBusy(false);
    }
  };

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
  }, [gameId, refreshKey]);

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
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border px-6">
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
          onClick={() => setScrapeOpen(true)}
          disabled={!game}
          className="h-8 gap-1.5"
        >
          <Sparkles className="size-3.5" />
          刮削
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setEditOpen(true)}
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
            <div className="border-b border-border">
              <div className={cn(PAGE, "flex items-end gap-7 pb-8 pt-10")}>
                <div className="aspect-[2/3] w-[176px] shrink-0 animate-pulse rounded-xl bg-muted" />
                <div className="flex-1 space-y-3 pb-2">
                  <div className="h-7 w-1/2 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-1/4 animate-pulse rounded bg-muted" />
                  <div className="h-3 w-1/3 animate-pulse rounded bg-muted" />
                </div>
                <div className="hidden w-[320px] shrink-0 grid-cols-2 gap-4 lg:grid">
                  {Array.from({ length: 6 }).map((_, i) => (
                    <div key={i} className="h-8 animate-pulse rounded bg-muted" />
                  ))}
                </div>
              </div>
            </div>
            <div className={cn(PAGE, "grid grid-cols-8 gap-3 py-8")}>
              {Array.from({ length: 8 }).map((_, i) => (
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
                  <>
                    <img
                      src={poster}
                      alt=""
                      className="h-full w-full scale-110 object-cover opacity-40 blur-2xl"
                    />
                    <div className="absolute inset-0 bg-gradient-to-r from-background/70 via-background/60 to-background/80" />
                    <div className="absolute inset-0 bg-gradient-to-b from-background/20 via-background/50 to-background" />
                  </>
                ) : (
                  <div className="h-full w-full bg-gradient-to-br from-primary/[0.10] to-transparent" />
                )}
              </div>

              <div className={cn(PAGE, "relative flex items-end gap-7 pb-8 pt-10")}>
                {/* 竖版封面 */}
                <div className="group relative aspect-[2/3] w-[176px] shrink-0 overflow-hidden rounded-xl shadow-[0_20px_44px_-20px_rgba(24,24,27,0.55)] ring-1 ring-black/10">
                  {poster ? (
                    <img
                      src={poster}
                      alt={game.titleCn || game.title}
                      className="h-full w-full object-cover"
                    />
                  ) : (
                    <div className="flex h-full w-full items-center justify-center bg-gradient-to-br from-primary/25 to-primary/5">
                      <span className="select-none font-serif text-[64px] leading-none text-white/80">
                        {(game.titleCn || game.title).slice(0, 1)}
                      </span>
                    </div>
                  )}

                  {/* 悬停显示换封面：封面属于"一眼就能判断好坏"的东西，值得一个就地入口 */}
                  <button
                    type="button"
                    onClick={() => setCoverPickerOpen(true)}
                    className="absolute inset-x-0 bottom-0 flex items-center justify-center gap-1 bg-gradient-to-t from-black/70 to-transparent px-2 pb-2 pt-6 text-[10.5px] text-white opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-visible:opacity-100"
                  >
                    <ImageIcon className="size-3" />
                    更换封面
                  </button>

                  {coverBusy && (
                    <div className="absolute inset-0 flex items-center justify-center bg-black/35">
                      <span className="text-[10.5px] text-white">处理中…</span>
                    </div>
                  )}
                </div>

                {/* 标题区 */}
                <div className="min-w-0 flex-1 pb-1">
                  <h1 className="text-[30px] font-semibold leading-[1.15] tracking-tight">
                    {game.titleCn || game.title}
                  </h1>
                  {(game.originalTitle || game.title) && (
                    <p className="mt-1.5 truncate text-[12.5px] text-muted-foreground">
                      {[game.originalTitle, game.title].filter(Boolean).join(" · ")}
                    </p>
                  )}

                  <div className="mt-4 flex flex-wrap items-center gap-2">
                    <span className="rounded-md bg-card/70 px-2 py-1 text-[11.5px] text-foreground ring-1 ring-border">
                      {game.developer || "会社未知"}
                    </span>
                    <span className="rounded-md bg-card/70 px-2 py-1 text-[11.5px] text-foreground ring-1 ring-border">
                      {game.released || "发售日未知"}
                    </span>
                    <span className="rounded-md bg-card/70 px-2 py-1 text-[11.5px] text-foreground ring-1 ring-border">
                      {engineLabel(game.engine)}
                    </span>
                    <span className="rounded-md bg-card/70 px-2 py-1 text-[11.5px] text-foreground ring-1 ring-border">
                      {PLATFORM_LABEL[game.platform]}
                    </span>
                    {game.completed && (
                      <span className="rounded-md bg-primary/10 px-2 py-1 text-[11.5px] font-medium text-primary">
                        已通关
                      </span>
                    )}
                  </div>

                  {game.tags.length > 0 && (
                    <div className="mt-3 flex flex-wrap gap-1.5">
                      {game.tags.slice(0, 14).map((t) => (
                        <span
                          key={t}
                          className="rounded-full border border-border bg-card/60 px-2 py-[1px] text-[10.5px] text-muted-foreground"
                        >
                          {t}
                        </span>
                      ))}
                      {game.tags.length > 14 && (
                        <span className="px-1 text-[10.5px] text-muted-foreground/70">
                          +{game.tags.length - 14}
                        </span>
                      )}
                    </div>
                  )}
                </div>

                {/* 右侧关键数据：填掉标题右边的空白带 */}
                <div className="hidden w-[320px] shrink-0 pb-1 lg:block">
                  <div className="rounded-xl border border-border bg-card/80 p-4 backdrop-blur-[2px]">
                    <div className="flex items-end justify-between gap-3">
                      <div>
                        <p className="text-[10.5px] text-muted-foreground">评分</p>
                        <div className="mt-0.5 flex items-baseline gap-1.5">
                          <Star className="size-3.5 translate-y-[-1px] fill-amber-400 text-amber-400" />
                          <span className="text-[22px] font-semibold leading-none tabular-nums">
                            {game.rating > 0 ? game.rating.toFixed(1) : "—"}
                          </span>
                          <span className="text-[10.5px] text-muted-foreground">/ 10</span>
                        </div>
                      </div>
                      {game.ratingSource && (
                        <span className="mb-0.5 rounded-full border border-border px-1.5 py-[1px] text-[9.5px] text-muted-foreground">
                          {game.ratingSource}
                        </span>
                      )}
                    </div>

                    <div className="mt-3.5 grid grid-cols-2 gap-x-5 gap-y-3 border-t border-border pt-3.5">
                      <Fact label="发售">{game.released ? game.released.slice(0, 4) : "—"}</Fact>
                      <Fact label="引擎">{engineLabel(game.engine)}</Fact>
                      <Fact label="文件数">
                        {game.fileCount > 0 ? `${formatCount(game.fileCount)} 个` : "—"}
                      </Fact>
                      <Fact label="体积">
                        {game.dirSize > 0 ? formatBytes(game.dirSize) : "未统计"}
                      </Fact>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            {/* ---------------------------------------------------------- 主体 */}
            <div className={cn(PAGE, "pt-7")}>
              <div className="grid grid-cols-1 gap-9 lg:grid-cols-[minmax(0,1fr)_320px]">
                {/* 左：简介 + 角色 */}
                <div className="min-w-0">
                  <section>
                    <SectionHeading>简介</SectionHeading>
                    <p
                      className={cn(
                        "selectable whitespace-pre-line text-[12.5px] leading-[1.9] text-muted-foreground",
                        // 宽窗口排两栏：1250px 一行排 90 多个汉字根本没法读，两栏各 45 字正合适
                        "lg:columns-2 lg:gap-x-12",
                        // 折 8 行：真实数据里简介常有 700~1500 字，全展开会把角色区推到两三屏之外。
                        // 多栏布局下 line-clamp 不起作用，改用限高 + overflow-hidden。
                        !descOpen &&
                          description.length > 320 &&
                          "max-h-[190px] overflow-hidden [mask-image:linear-gradient(to_bottom,black_86%,transparent)]",
                      )}
                    >
                      {description ||
                        "这部游戏还没有简介（旧库刮削时没抓到，或本身就是没收录的条目）。"}
                    </p>
                    {description.length > 320 && (
                      <button
                        type="button"
                        onClick={() => setDescOpen((v) => !v)}
                        className="mt-1.5 text-[11px] text-primary transition-opacity hover:opacity-75"
                      >
                        {descOpen ? "收起" : "展开全部"}
                      </button>
                    )}
                  </section>

                  <section className="mt-9">
                    <SectionHeading
                      right={
                        <p className="text-[11px] tabular-nums text-muted-foreground">
                          {characters.length > 0
                            ? `共 ${formatCount(characters.length)} 位${
                                withCv > 0 ? ` · ${withCv} 位有配音` : ""
                              }`
                            : ""}
                        </p>
                      }
                    >
                      <span className="flex items-center gap-1.5">
                        <Users className="size-3.5 text-muted-foreground" />
                        角色
                      </span>
                    </SectionHeading>

                    {characters.length === 0 ? (
                      <p className="rounded-xl border border-dashed border-border px-4 py-10 text-center text-[11.5px] leading-relaxed text-muted-foreground">
                        这部还没有角色数据。
                        <br />
                        旧库刮削时只抓了 196 部，其余条目要等 D 阶段接上刮削后才有。
                      </p>
                    ) : (
                      <ul className="grid grid-cols-[repeat(auto-fill,minmax(126px,1fr))] gap-x-3.5 gap-y-5">
                        {characters.map((c) => (
                          <CharacterCard key={`${c.name}-${c.sortOrder}`} c={c} />
                        ))}
                      </ul>
                    )}
                  </section>
                </div>

                {/* 右：文件信息，跟着滚动吸顶 */}
                <aside className="min-w-0">
                  <div className="lg:sticky lg:top-0">
                    <SectionHeading>文件</SectionHeading>
                    <div className="rounded-xl border border-border bg-card px-3.5 py-1">
                      <MetaRow label="目录" value={game.folderPath} title={game.folderPath} />
                      <MetaRow
                        label="主程序"
                        value={game.launchExe || "未识别"}
                        title={game.launchExe}
                      />
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
                  </div>
                </aside>
              </div>
            </div>

            <div className="h-12" />
          </>
        )}
      </div>

      {game && (
        <>
          <GameEditDialog
            open={editOpen}
            game={game}
            onOpenChange={setEditOpen}
            onSaved={() => refresh()}
          />
          <ScrapeDialog
            open={scrapeOpen}
            game={game}
            onOpenChange={setScrapeOpen}
            onApplied={(_game, report: ScrapeReport) => {
              refresh();
              // 刮削会改很多字段，用一条系统提示让用户知道"哪些没被改"
              if (report.skippedLocked.length > 0) {
                toast("部分字段已锁定，未被覆盖", {
                  description: report.skippedLocked.join("、"),
                });
              }
            }}
          />
          <PathPickerDialog
            open={coverPickerOpen}
            mode="image"
            title="选择封面图片"
            onOpenChange={setCoverPickerOpen}
            onPick={(path) => void pickCover(path)}
          />
        </>
      )}
    </div>
  );
}
