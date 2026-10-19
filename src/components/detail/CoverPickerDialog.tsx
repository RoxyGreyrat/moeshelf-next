import { useEffect, useState } from "react";
import { Check, Link2, Loader2, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { coverCandidates, setCoverFromUrl } from "@/lib/api";
import type { CoverOption } from "@/lib/api";
import type { Game } from "@/types/game";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

interface CoverPickerDialogProps {
  open: boolean;
  game: Game;
  onOpenChange: (open: boolean) => void;
  onPicked: (game: Game) => void;
}

/**
 * 从已关联的 VNDB 条目里挑封面。
 *
 * 不按标题重新搜索 —— 搜出来的东西里混着同名重制版、外传和无关条目，
 * 点一张就换错了。库里记着这部作品的 `vndbId`，直接拿那一条的图：
 * 主封面一张，加上每个发售版本（初回限定 / 通常版 / 数字版…）各自的包装图。
 */
export function CoverPickerDialog({
  open,
  game,
  onOpenChange,
  onPicked,
}: CoverPickerDialogProps) {
  const [list, setList] = useState<CoverOption[] | null>(null);
  const [entry, setEntry] = useState<{ vndbId: string; title: string } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyUrl, setBusyUrl] = useState<string | null>(null);

  const vndbId = game.vndbId?.trim() ?? "";

  useEffect(() => {
    if (!open) return;

    // 没关联条目就直接给引导，不发请求 —— 少一次必然失败的等待
    if (!vndbId) {
      setList(null);
      setEntry(null);
      setError(null);
      setLoading(false);
      return;
    }

    let alive = true;
    setLoading(true);
    setError(null);
    setList(null);
    setEntry(null);

    coverCandidates(game.id, 30)
      .then((res) => {
        if (!alive) return;
        setEntry({ vndbId: res.vndbId, title: res.title });
        setList(res.options);
      })
      .catch((err) => {
        console.error("[cover_candidates]", err);
        if (alive) {
          setError(String(err));
          setList([]);
        }
      })
      .finally(() => {
        if (alive) setLoading(false);
      });

    return () => {
      alive = false;
    };
  }, [open, game.id, vndbId]);

  const pick = async (opt: CoverOption) => {
    setBusyUrl(opt.url);
    try {
      const next = await setCoverFromUrl(game.id, opt.url);
      onPicked(next);
      toast.success("封面已更新");
      onOpenChange(false);
    } catch (err) {
      console.error("[set_cover_from_url]", err);
      toast.error("换封面失败", { description: String(err) });
    } finally {
      setBusyUrl(null);
    }
  };

  const current = game.coverUrl?.trim() ?? "";

  /** 未关联 VNDB 条目时的引导 */
  const renderUnlinked = () => (
    <div className="flex h-[260px] flex-col items-center justify-center gap-2 px-8 text-center">
      <span className="flex size-9 items-center justify-center rounded-full bg-muted text-muted-foreground">
        <Link2 className="size-4" />
      </span>
      <p className="text-[12.5px] font-medium">这部还没关联 VNDB 条目</p>
      <p className="max-w-[420px] text-[11.5px] leading-relaxed text-muted-foreground">
        先点「获取资料」，在 VNDB 结果里选一条应用一次；
        之后这里会直接列出该条目自带的全部封面（含各发售版本的包装图）。
      </p>
    </div>
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-[760px]">
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="text-[15px]">选择封面</DialogTitle>
          <DialogDescription className="text-[11.5px] leading-relaxed">
            {vndbId
              ? "这些图都来自这部作品已关联的 VNDB 条目，不同发售版本各有自己的包装图。点一张就会下载并替换。"
              : "封面候选只从已关联的 VNDB 条目里取，不再按标题搜索，避免换到同名作品上。"}
          </DialogDescription>
          {entry && (
            <p className="mt-1.5 flex items-center gap-1.5 text-[11px] text-muted-foreground">
              <Link2 className="size-3 shrink-0" />
              <span className="truncate">
                VNDB {entry.vndbId} · {entry.title}
              </span>
            </p>
          )}
        </DialogHeader>

        <div className="max-h-[480px] min-h-[260px] overflow-y-auto px-4 py-4">
          {!vndbId ? (
            renderUnlinked()
          ) : loading ? (
            <div className="flex h-[260px] flex-col items-center justify-center gap-2 text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
              <p className="text-[11.5px]">正在读取条目封面…</p>
            </div>
          ) : error ? (
            <div className="flex h-[260px] flex-col items-center justify-center gap-2 px-6 text-center">
              <p className="text-[12.5px] font-medium">没能取到封面候选</p>
              <p className="max-w-[440px] break-all text-[11px] leading-relaxed text-muted-foreground">
                {error}
              </p>
              <Button
                variant="outline"
                size="sm"
                className="mt-1 h-7 gap-1.5 text-[11.5px]"
                onClick={() => {
                  // 重新触发一次 effect
                  onOpenChange(false);
                  onOpenChange(true);
                }}
              >
                <RefreshCw className="size-3" />
                重试
              </Button>
            </div>
          ) : list && list.length === 0 ? (
            <div className="flex h-[260px] flex-col items-center justify-center gap-1 px-6 text-center">
              <p className="text-[12.5px] font-medium">这个条目没有可用的图片</p>
              <p className="text-[11px] text-muted-foreground">
                可以在上面用本地图片换封面。
              </p>
            </div>
          ) : (
            <ul className="grid grid-cols-[repeat(auto-fill,minmax(118px,1fr))] gap-3">
              {list?.map((opt) => {
                const active = current && opt.url === current;
                return (
                  <li key={opt.url}>
                    <button
                      type="button"
                      disabled={busyUrl !== null}
                      onClick={() => void pick(opt)}
                      title={opt.label}
                      className={cn(
                        "group block w-full overflow-hidden rounded-lg bg-muted text-left ring-1 transition-all duration-150",
                        active
                          ? "ring-2 ring-primary"
                          : "ring-black/[0.06] hover:ring-primary/50",
                      )}
                    >
                      <div className="relative aspect-[2/3] overflow-hidden">
                        <img
                          src={opt.url}
                          alt={opt.label}
                          loading="lazy"
                          className="h-full w-full object-cover transition-transform duration-200 group-hover:scale-[1.03]"
                        />
                        {opt.kind && (
                          <span className="absolute left-1 top-1 rounded bg-black/55 px-1.5 py-0.5 text-[9.5px] leading-none text-white backdrop-blur-sm">
                            {opt.kind}
                          </span>
                        )}
                        {active && (
                          <span className="absolute right-1 top-1 flex size-5 items-center justify-center rounded-full bg-primary text-primary-foreground">
                            <Check className="size-3" />
                          </span>
                        )}
                        {busyUrl === opt.url && (
                          <span className="absolute inset-0 flex items-center justify-center bg-background/70">
                            <Loader2 className="size-4 animate-spin text-muted-foreground" />
                          </span>
                        )}
                      </div>
                      <p className="truncate px-1.5 py-1 text-[10.5px] text-muted-foreground">
                        {opt.label}
                      </p>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="flex items-center justify-end border-t border-border px-5 py-3">
          <Button variant="outline" size="sm" className="h-8" onClick={() => onOpenChange(false)}>
            关闭
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
