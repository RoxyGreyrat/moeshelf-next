import { useCallback, useEffect, useState } from "react";
import { Check, Loader2, RefreshCw, Search, Sparkles, Star } from "lucide-react";
import { toast } from "sonner";

import { vndbApply, vndbSearch } from "@/lib/api";
import type { ScrapeReport, VnCandidate } from "@/lib/api";
import type { Game } from "@/types/game";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

interface ScrapeDialogProps {
  open: boolean;
  game: Game;
  onOpenChange: (open: boolean) => void;
  onApplied: (game: Game, report: ScrapeReport) => void;
}

/**
 * VNDB 刮削：搜候选 → 人工挑一条 → 应用。
 *
 * 刻意**不做全自动匹配**：galgame 的标题（尤其汉化名）和 VNDB 的罗马字/日文名
 * 经常对不上，自动挑错一次就会把整部游戏的标题、会社、封面全改错，
 * 而这些都是"锁定后才不会被覆盖"的字段，改错要手动一项项修。
 * 让用户看一眼缩略图再点，成本低得多。
 */
export function ScrapeDialog({
  open,
  game,
  onOpenChange,
  onApplied,
}: ScrapeDialogProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<VnCandidate[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applyingId, setApplyingId] = useState<string | null>(null);

  const runSearch = useCallback(async (q: string) => {
    const text = q.trim();
    if (!text) return;
    setSearching(true);
    setError(null);
    try {
      setResults(await vndbSearch(text, 10));
    } catch (err) {
      console.error("[vndb_search]", err);
      setError(String(err));
      setResults([]);
    } finally {
      setSearching(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    // 默认用中文标题搜；没有中文名就用外文名
    const initial = (game.titleCn || game.title || game.originalTitle || "").trim();
    setQuery(initial);
    setResults(null);
    setError(null);
    if (initial) void runSearch(initial);
  }, [open, game, runSearch]);

  const apply = async (c: VnCandidate) => {
    setApplyingId(c.id);
    try {
      const report = await vndbApply(game.id, c.id);
      const parts = [`更新 ${report.updatedFields.length} 个字段`];
      if (report.coverSaved) parts.push("封面已下载");
      if (report.characters > 0) parts.push(`角色 ${report.characters} 位`);
      if (report.skippedLocked.length > 0)
        parts.push(`跳过已锁定 ${report.skippedLocked.length} 项`);

      toast.success(`已应用《${c.titleCn || c.title}》`, { description: parts.join(" · ") });
      onApplied(report.game, report);
      onOpenChange(false);
    } catch (err) {
      console.error("[vndb_apply]", err);
      toast.error("刮削失败", { description: String(err) });
    } finally {
      setApplyingId(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-[680px]">
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="flex items-center gap-1.5 text-[15px]">
            <Sparkles className="size-4 text-primary" />
            从 VNDB 刮削
          </DialogTitle>
          <DialogDescription className="text-[11.5px]">
            搜到的是候选列表，请你确认是哪一部再应用。已锁定的字段不会被覆盖。
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-2 border-b border-border px-4 py-2.5">
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void runSearch(query);
            }}
            placeholder="用日文原名或罗马字更容易搜到"
            className="h-8 text-[12.5px]"
          />
          <Button
            size="sm"
            className="h-8 shrink-0 gap-1.5"
            disabled={searching || !query.trim()}
            onClick={() => void runSearch(query)}
          >
            {searching ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Search className="size-3.5" />
            )}
            搜索
          </Button>
        </div>

        <div className="max-h-[420px] min-h-[220px] overflow-y-auto px-2 py-1.5">
          {searching ? (
            <div className="flex h-[220px] flex-col items-center justify-center gap-2 text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
              <p className="text-[11.5px]">正在查 VNDB…</p>
            </div>
          ) : error ? (
            <div className="flex h-[220px] flex-col items-center justify-center gap-2 px-6 text-center">
              <p className="text-[12.5px] font-medium">搜索失败</p>
              <p className="max-w-[420px] break-all text-[11px] leading-relaxed text-muted-foreground">
                {error}
              </p>
              <Button
                variant="outline"
                size="sm"
                className="mt-1 h-7 gap-1.5 text-[11.5px]"
                onClick={() => void runSearch(query)}
              >
                <RefreshCw className="size-3" />
                重试
              </Button>
            </div>
          ) : results && results.length === 0 ? (
            <div className="flex h-[220px] flex-col items-center justify-center gap-1 px-6 text-center">
              <p className="text-[12.5px] font-medium">没有搜到</p>
              <p className="text-[11px] text-muted-foreground">
                换个写法试试：日文原名（千恋＊万花）或罗马字（Senren Banka）。
              </p>
            </div>
          ) : results ? (
            <ul className="space-y-1">
              {results.map((c) => (
                <CandidateRow
                  key={c.id}
                  c={c}
                  busy={applyingId === c.id}
                  disabled={applyingId !== null}
                  onApply={() => void apply(c)}
                />
              ))}
            </ul>
          ) : null}
        </div>

        <div className="flex items-center justify-between border-t border-border px-5 py-3">
          <p className="text-[10.5px] text-muted-foreground">
            数据来自 VNDB（kana API），请求有节流。
          </p>
          <Button variant="outline" size="sm" className="h-8" onClick={() => onOpenChange(false)}>
            关闭
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function CandidateRow({
  c,
  busy,
  disabled,
  onApply,
}: {
  c: VnCandidate;
  busy: boolean;
  disabled: boolean;
  onApply: () => void;
}) {
  const [broken, setBroken] = useState(false);
  const year = c.released ? c.released.slice(0, 4) : "";

  return (
    <li className="flex items-center gap-3 rounded-lg px-2 py-2 transition-colors hover:bg-muted/50">
      <div className="aspect-[2/3] w-[42px] shrink-0 overflow-hidden rounded-md bg-muted ring-1 ring-black/5">
        {c.imageUrl && !broken ? (
          <img
            src={c.imageUrl}
            alt=""
            loading="lazy"
            onError={() => setBroken(true)}
            className="h-full w-full object-cover"
          />
        ) : null}
      </div>

      <div className="min-w-0 flex-1">
        <p className="truncate text-[12.5px] font-medium text-foreground">
          {c.titleCn || c.title}
          {c.titleCn && (
            <span className="ml-1.5 text-[11px] font-normal text-muted-foreground">
              {c.title}
            </span>
          )}
        </p>
        <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
          {c.altTitle || "—"}
        </p>
        <p className="mt-0.5 flex items-center gap-2 text-[10.5px] text-muted-foreground">
          {year && <span>{year}</span>}
          {c.developer && <span className="truncate">{c.developer}</span>}
          {c.rating > 0 && (
            <span className="flex items-center gap-0.5">
              <Star className="size-2.5 fill-amber-400 text-amber-400" />
              {c.rating.toFixed(1)}
            </span>
          )}
          <span className="text-muted-foreground/60">{c.id}</span>
        </p>
      </div>

      <Button
        size="sm"
        variant="outline"
        className="h-7 shrink-0 gap-1 px-2 text-[11.5px]"
        disabled={disabled}
        onClick={onApply}
      >
        {busy ? <Loader2 className="size-3 animate-spin" /> : <Check className="size-3" />}
        应用
      </Button>
    </li>
  );
}
