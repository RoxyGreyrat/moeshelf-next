import { useState } from "react";
import { X } from "lucide-react";
import type { Engine, Filters, Platform } from "@/types/game";
import { ENGINE_LABEL, PLATFORM_LABEL } from "@/types/game";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

/** 标签云默认展示数量，超出的收起 */
const TAG_PREVIEW = 12;

interface FilterSidebarProps {
  filters: Filters;
  platformCounts: Record<Platform, number>;
  engineCounts: Record<Engine, number>;
  tagCounts: Record<string, number>;
  allTags: string[];
  activeCount: number;
  onTogglePlatform: (p: Platform) => void;
  onToggleEngine: (e: Engine) => void;
  onToggleTag: (t: string) => void;
  onClearAll: () => void;
}

function GroupTitle({ children, count }: { children: React.ReactNode; count?: number }) {
  return (
    <div className="mb-2 flex items-baseline justify-between">
      <span className="text-[11px] font-semibold tracking-[0.08em] text-muted-foreground">
        {children}
      </span>
      {typeof count === "number" && (
        <span className="text-[10px] tabular-nums text-muted-foreground/70">
          {count}
        </span>
      )}
    </div>
  );
}

function CheckRow({
  id,
  label,
  count,
  checked,
  onCheckedChange,
}: {
  id: string;
  label: string;
  count: number;
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
}) {
  return (
    <label
      htmlFor={id}
      className={cn(
        "flex cursor-pointer items-center gap-2.5 rounded-md px-1.5 py-[7px] transition-colors",
        "hover:bg-muted/70",
        count === 0 && "opacity-45",
      )}
    >
      <Checkbox
        id={id}
        checked={checked}
        onCheckedChange={(v) => onCheckedChange(v === true)}
        className="size-4"
      />
      <span className="flex-1 truncate text-[13px] text-foreground">{label}</span>
      <span className="text-[11px] tabular-nums text-muted-foreground">{count}</span>
    </label>
  );
}

export function FilterSidebar({
  filters,
  platformCounts,
  engineCounts,
  tagCounts,
  allTags,
  activeCount,
  onTogglePlatform,
  onToggleEngine,
  onToggleTag,
  onClearAll,
}: FilterSidebarProps) {
  const platforms = Object.keys(PLATFORM_LABEL) as Platform[];
  const engines = Object.keys(ENGINE_LABEL) as Engine[];
  // 标签只显示库里真实出现过的，按出现次数排序；默认只露 12 个，避免侧栏变成一堵墙
  const [showAllTags, setShowAllTags] = useState(false);
  const allVisibleTags = allTags
    .filter((t) => (tagCounts[t] ?? 0) > 0)
    .sort((a, b) => (tagCounts[b] ?? 0) - (tagCounts[a] ?? 0));
  const visibleTags = showAllTags
    ? allVisibleTags
    : allVisibleTags.slice(0, TAG_PREVIEW);

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-background">
      <div className="flex items-center justify-between px-4 pb-1 pt-4">
        <span className="text-[13px] font-semibold tracking-tight">筛选</span>
        {activeCount > 0 && (
          <Badge
            variant="secondary"
            className="h-5 rounded-full px-1.5 text-[10px] tabular-nums"
          >
            {activeCount}
          </Badge>
        )}
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="space-y-5 px-3 pb-4 pt-2">
          <section>
            <GroupTitle>平台</GroupTitle>
            <div className="space-y-0.5">
              {platforms.map((p) => (
                <CheckRow
                  key={p}
                  id={`platform-${p}`}
                  label={PLATFORM_LABEL[p]}
                  count={platformCounts[p] ?? 0}
                  checked={filters.platforms.includes(p)}
                  onCheckedChange={() => onTogglePlatform(p)}
                />
              ))}
            </div>
          </section>

          <section>
            <GroupTitle>引擎</GroupTitle>
            <div className="space-y-0.5">
              {engines.map((e) => (
                <CheckRow
                  key={e}
                  id={`engine-${e}`}
                  label={ENGINE_LABEL[e]}
                  count={engineCounts[e] ?? 0}
                  checked={filters.engines.includes(e)}
                  onCheckedChange={() => onToggleEngine(e)}
                />
              ))}
            </div>
          </section>

          <section>
            <GroupTitle count={filters.tags.length || undefined}>标签</GroupTitle>
            <div className="flex flex-wrap gap-1.5">
              {visibleTags.map((tag) => {
                const active = filters.tags.includes(tag);
                return (
                  <button
                    key={tag}
                    type="button"
                    onClick={() => onToggleTag(tag)}
                    aria-pressed={active}
                    className={cn(
                      "inline-flex items-center gap-1 rounded-full border px-2.5 py-[3px] text-[12px] leading-[18px] transition-colors duration-150",
                      active
                        ? "border-primary/25 bg-accent text-accent-foreground"
                        : "border-border bg-card text-muted-foreground hover:border-zinc-300 hover:bg-muted/60 hover:text-foreground",
                    )}
                  >
                    {tag}
                    {active && <X className="size-3 opacity-70" />}
                  </button>
                );
              })}
            </div>
            {allVisibleTags.length > TAG_PREVIEW && (
              <button
                type="button"
                onClick={() => setShowAllTags((v) => !v)}
                className="mt-2 text-[11.5px] text-primary transition-opacity hover:opacity-75"
              >
                {showAllTags ? "收起标签" : `展开全部 ${allVisibleTags.length} 个`}
              </button>
            )}
          </section>
        </div>
      </ScrollArea>

      <div className="border-t border-border p-2">
        <Button
          variant="ghost"
          size="sm"
          disabled={activeCount === 0}
          onClick={onClearAll}
          className="h-8 w-full justify-start gap-1.5 px-2 text-[12px] font-normal text-muted-foreground hover:text-foreground disabled:opacity-40"
        >
          <X className="size-3.5" />
          清除全部筛选
        </Button>
      </div>
    </aside>
  );
}
