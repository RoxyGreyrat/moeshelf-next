import { useEffect, useState } from "react";
import { Check, Loader2, RefreshCw } from "lucide-react";
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
 * 从 VNDB 挑封面。
 *
 * VNDB 一部作品只有一张官方封面，所谓"换一张"实际是在**同一作品的不同版本、
 * 重制版、外传**之间挑（旧项目也是这么做的）。所以这里按标题搜一批作品，
 * 把它们的封面平铺出来，点哪张用哪张。
 */
export function CoverPickerDialog({
  open,
  game,
  onOpenChange,
  onPicked,
}: CoverPickerDialogProps) {
  const [list, setList] = useState<CoverOption[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyUrl, setBusyUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    const query = (game.titleCn || game.title || game.originalTitle || "").trim();
    if (!query) {
      setError("这部没有可用的标题，无法搜索封面");
      setList([]);
      return;
    }

    setLoading(true);
    setError(null);
    setList(null);

    coverCandidates(query, 12)
      .then((items) => {
        if (alive) setList(items);
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
  }, [open, game]);

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

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-[720px]">
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="text-[15px]">从 VNDB 选封面</DialogTitle>
          <DialogDescription className="text-[11.5px]">
            同一作品的不同版本会各自有封面，点一张就会下载并替换（可再换回来）。
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[460px] min-h-[240px] overflow-y-auto px-4 py-4">
          {loading ? (
            <div className="flex h-[240px] flex-col items-center justify-center gap-2 text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
              <p className="text-[11.5px]">正在查 VNDB…</p>
            </div>
          ) : error ? (
            <div className="flex h-[240px] flex-col items-center justify-center gap-2 px-6 text-center">
              <p className="text-[12.5px] font-medium">没能取到封面候选</p>
              <p className="max-w-[440px] break-all text-[11px] leading-relaxed text-muted-foreground">
                {error}
              </p>
              <Button
                variant="outline"
                size="sm"
                className="mt-1 h-7 gap-1.5 text-[11.5px]"
                onClick={() => {
                  onOpenChange(false);
                  onOpenChange(true);
                }}
              >
                <RefreshCw className="size-3" />
                重试
              </Button>
            </div>
          ) : list && list.length === 0 ? (
            <div className="flex h-[240px] flex-col items-center justify-center gap-1 px-6 text-center">
              <p className="text-[12.5px] font-medium">没有搜到带封面的条目</p>
              <p className="text-[11px] text-muted-foreground">
                可以在「编辑信息」里改用本地图片。
              </p>
            </div>
          ) : (
            <ul className="grid grid-cols-[repeat(auto-fill,minmax(112px,1fr))] gap-3">
              {list?.map((opt) => {
                const active = current && opt.url === current;
                return (
                  <li key={opt.url}>
                    <button
                      type="button"
                      disabled={busyUrl !== null}
                      onClick={() => void pick(opt)}
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
