import { useCallback, useEffect, useState } from "react";
import {
  Check,
  CloudDownload,
  Loader2,
  RefreshCw,
  Search,
  Star,
} from "lucide-react";
import { toast } from "sonner";

import { sourceApply, sourceSearch } from "@/lib/api";
import type { Candidate, ScrapeReport } from "@/lib/api";
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
import { cn } from "@/lib/utils";

/** 三个信息源。id 与 Rust 侧 `scrape::search` 的分发键一致 */
const SOURCES = [
  { id: "vndb", label: "VNDB", hint: "角色最全，有官方中文名" },
  { id: "bangumi", label: "Bangumi", hint: "中文简介，角色带声优" },
  { id: "ymgal", label: "月幕", hint: "中文简介与中文名" },
] as const;

interface FetchInfoDialogProps {
  open: boolean;
  game: Game;
  onOpenChange: (open: boolean) => void;
  onApplied: (game: Game, report: ScrapeReport) => void;
}

/**
 * 「获取资料」：挑一个信息源 → 搜候选 → 人工确认 → 应用。
 *
 * 刻意不做自动匹配：汉化名和各站的日文/罗马字名经常对不上，
 * 自动挑错一次会把标题、会社、封面全改错，而这些都是"锁定后不再被覆盖"的字段。
 */
export function FetchInfoDialog({
  open,
  game,
  onOpenChange,
  onApplied,
}: FetchInfoDialogProps) {
  const [source, setSource] = useState<string>("vndb");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Candidate[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applyingId, setApplyingId] = useState<string | null>(null);

  const runSearch = useCallback(
    async (src: string, q: string) => {
      const text = q.trim();
      if (!text) return;
      setSearching(true);
      setError(null);
      setResults(null);
      try {
        setResults(await sourceSearch(src, text, 10));
      } catch (err) {
        console.error("[source_search]", err);
        setError(String(err));
        setResults([]);
      } finally {
        setSearching(false);
      }
    },
    [],
  );

  useEffect(() => {
    if (!open) return;
    const initial = (game.titleCn || game.title || game.originalTitle || "").trim();
    setQuery(initial);
    setSource("vndb");
    setError(null);
    setResults(null);
    if (initial) void runSearch("vndb", initial);
  }, [open, game, runSearch]);

  const switchSource = (next: string) => {
    if (next === source) return;
    setSource(next);
    void runSearch(next, query);
  };

  const apply = async (c: Candidate) => {
    setApplyingId(c.id);
    try {
      const report = await sourceApply(game.id, source, c.id);
      const parts = [`更新 ${report.updatedFields.length} 项资料`];
      if (report.coverSaved) parts.push("封面已保存");
      if (report.characters > 0) parts.push(`角色 ${report.characters} 位`);
      if (report.skippedLocked.length > 0)
        parts.push(`跳过已锁定 ${report.skippedLocked.length} 项`);

      toast.success(`已应用《${c.titleCn || c.title}》`, {
        description: parts.join(" · "),
      });
      onApplied(report.game, report);
      onOpenChange(false);
    } catch (err) {
      console.error("[source_apply]", err);
      toast.error("获取失败", { description: String(err) });
    } finally {
      setApplyingId(null);
    }
  };

  const active = SOURCES.find((s) => s.id === source);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-[700px]">
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="flex items-center gap-1.5 text-[15px]">
            <CloudDownload className="size-4 text-primary" />
            获取游戏资料
          </DialogTitle>
          <DialogDescription className="text-[11.5px]">
            从资料站查出候选，你确认是哪一部后再应用。已锁定的字段不会被覆盖。
          </DialogDescription>
        </DialogHeader>

        {/* 信息源切换 */}
        <div className="flex items-center gap-1.5 border-b border-border px-4 py-2">
          {SOURCES.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() => switchSource(s.id)}
              title={s.hint}
              className={cn(
                "rounded-md px-2.5 py-1 text-[11.5px] transition-colors duration-150",
                source === s.id
                  ? "bg-accent font-medium text-accent-foreground"
                  : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
              )}
            >
              {s.label}
            </button>
          ))}
          <span className="ml-auto truncate text-[10.5px] text-muted-foreground/70">
            {active?.hint}
          </span>
        </div>

        <div className="flex items-center gap-2 border-b border-border px-4 py-2.5">
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void runSearch(source, query);
            }}
            placeholder="用日文原名或罗马字更容易搜到"
            className="h-8 text-[12.5px]"
          />
          <Button
            size="sm"
            className="h-8 shrink-0 gap-1.5"
            disabled={searching || !query.trim()}
            onClick={() => void runSearch(source, query)}
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
              <p className="text-[11.5px]">正在查询 {active?.label}…</p>
            </div>
          ) : error ? (
            <div className="flex h-[220px] flex-col items-center justify-center gap-2 px-6 text-center">
              <p className="text-[12.5px] font-medium">这个源暂时连不上</p>
              <p className="max-w-[440px] break-all text-[11px] leading-relaxed text-muted-foreground">
                {error}
              </p>
              <div className="mt-1 flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 gap-1.5 text-[11.5px]"
                  onClick={() => void runSearch(source, query)}
                >
                  <RefreshCw className="size-3" />
                  重试
                </Button>
                {source !== "vndb" && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 text-[11.5px]"
                    onClick={() => switchSource("vndb")}
                  >
                    改用 VNDB
                  </Button>
                )}
              </div>
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
                  key={`${c.source}-${c.id}`}
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
            数据来自 {active?.label}，请求有节流。
          </p>
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            onClick={() => onOpenChange(false)}
          >
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
  c: Candidate;
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
          {c.titleCn && c.title !== c.titleCn && (
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
