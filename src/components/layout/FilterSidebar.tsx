import { useMemo, useState } from "react";
import { ChevronDown, Search, X } from "lucide-react";

import type { Engine, Filters, Platform } from "@/types/game";
import { engineLabel, PLATFORM_LABEL } from "@/types/game";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

/**
 * 左侧筛选面板 —— 参照 Wallpaper Engine 的「筛选」：
 *
 * - 每个分类是一个可折叠的分组（会社 / 引擎 / 标签 / 平台），标题右侧带数量；
 * - 每行是「复选框 + 名称 + 该值下的条目数」，数量是判断"这个筛选值值不值得点"的唯一依据；
 * - 顶部一个搜索框，值多了（630 个标签）靠它找，不靠滚动；
 * - 每组默认只露 8 行，点「显示全部」才展开 —— 否则标签组会把整个侧栏撑成一面墙。
 *
 * 为什么把标签从"胶囊云"改成"列表行"：胶囊云看不出数量、也没法搜索，
 * 标签一多就只是花花绿绿一片；列表行有数量、能搜索、和会社/引擎用同一套交互。
 */
const COLLAPSED_LIMIT = 8;

/** 搜索时的结果上限：630 个标签全渲染会明显卡，有搜索词时也没必要全给 */
const SEARCH_LIMIT = 120;

interface Option {
  value: string;
  label: string;
  count: number;
}

interface FilterSidebarProps {
  filters: Filters;
  platformCounts: Record<Platform, number>;
  developerCounts: Record<string, number>;
  engineCounts: Record<string, number>;
  tagCounts: Record<string, number>;
  allTags: string[];
  activeCount: number;
  onTogglePlatform: (p: Platform) => void;
  onToggleDeveloper: (d: string) => void;
  onToggleEngine: (e: Engine) => void;
  onToggleTag: (t: string) => void;
  onClearAll: () => void;
}

function buildOptions(
  values: Option[],
  selected: string[],
  query: string,
): Option[] {
  const q = query.trim().toLowerCase();
  const matched = q
    ? values.filter((o) => o.label.toLowerCase().includes(q))
    : values.slice();

  // 选中的永远置顶：选了 5 个标签之后，还得在 630 行里找它们就太蠢了
  return matched.sort((a, b) => {
    const sa = selected.includes(a.value) ? 1 : 0;
    const sb = selected.includes(b.value) ? 1 : 0;
    if (sa !== sb) return sb - sa;
    if (a.count !== b.count) return b.count - a.count;
    return a.label.localeCompare(b.label, "zh-Hans-CN", { numeric: true });
  });
}

function FilterGroup({
  title,
  options,
  selected,
  onToggle,
  searching,
  defaultOpen = true,
}: {
  title: string;
  options: Option[];
  selected: string[];
  onToggle: (value: string) => void;
  /** 有搜索词时自动展开、显示全部匹配项 */
  searching: boolean;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const [expanded, setExpanded] = useState(false);

  if (options.length === 0) return null;

  // 有搜索词时强制展开：搜索的结果如果藏在折叠组里，等于没搜
  const isOpen = searching || open;
  const chosen = options.filter((o) => selected.includes(o.value)).length;
  // 展开后不受上限限制；搜索时先只给前 120 条，点「显示全部」再看剩下的
  const limit = expanded ? options.length : searching ? SEARCH_LIMIT : COLLAPSED_LIMIT;
  const shown = options.slice(0, limit);
  const hidden = options.length - shown.length;

  return (
    <section className="border-b border-border/70 pb-2 last:border-b-0">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={isOpen}
        className="flex w-full items-center gap-1.5 rounded-md px-1.5 py-2 text-left transition-colors hover:bg-muted/60"
      >
        <ChevronDown
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform duration-150",
            !isOpen && "-rotate-90",
          )}
        />
        <span className="flex-1 text-[12.5px] font-semibold tracking-tight text-foreground">
          {title}
        </span>
        {chosen > 0 && (
          <Badge
            variant="secondary"
            className="h-[18px] rounded-full bg-accent px-1.5 text-[10px] tabular-nums text-accent-foreground"
          >
            {chosen}
          </Badge>
        )}
        <span className="text-[10.5px] tabular-nums text-muted-foreground/70">
          {options.length}
        </span>
      </button>

      {isOpen && (
        <div className="space-y-0.5 pb-1">
          {shown.map((o) => {
            const id = `${title}-${o.value || "__empty"}`;
            const checked = selected.includes(o.value);
            return (
              <label
                key={o.value || "__empty"}
                htmlFor={id}
                className={cn(
                  "flex cursor-pointer items-center gap-2.5 rounded-md py-[5px] pl-1.5 pr-2 transition-colors hover:bg-muted/70",
                  o.count === 0 && "opacity-45",
                )}
              >
                <Checkbox
                  id={id}
                  checked={checked}
                  onCheckedChange={() => onToggle(o.value)}
                  className="size-[15px]"
                />
                <span
                  className={cn(
                    "flex-1 truncate text-[12.5px]",
                    checked ? "font-medium text-foreground" : "text-foreground/90",
                  )}
                  title={o.label}
                >
                  {o.label}
                </span>
                <span className="text-[11px] tabular-nums text-muted-foreground/80">
                  {o.count}
                </span>
              </label>
            );
          })}

          {hidden > 0 && (
            <button
              type="button"
              onClick={() => setExpanded(true)}
              className="mt-1 pl-1.5 text-[11.5px] text-primary transition-opacity hover:opacity-75"
            >
              显示全部 {options.length} 个
            </button>
          )}
          {!hidden && expanded && !searching && options.length > COLLAPSED_LIMIT && (
            <button
              type="button"
              onClick={() => setExpanded(false)}
              className="mt-1 pl-1.5 text-[11.5px] text-muted-foreground transition-opacity hover:opacity-75"
            >
              收起
            </button>
          )}
        </div>
      )}
    </section>
  );
}

export function FilterSidebar({
  filters,
  platformCounts,
  developerCounts,
  engineCounts,
  tagCounts,
  allTags,
  activeCount,
  onTogglePlatform,
  onToggleDeveloper,
  onToggleEngine,
  onToggleTag,
  onClearAll,
}: FilterSidebarProps) {
  const [search, setSearch] = useState("");
  const searching = search.trim().length > 0;

  const platformOptions = useMemo<Option[]>(
    () =>
      (Object.keys(PLATFORM_LABEL) as Platform[])
        .map((p) => ({
          value: p,
          label: PLATFORM_LABEL[p],
          count: platformCounts[p] ?? 0,
        }))
        .filter((o) => o.count > 0),
    [platformCounts],
  );

  const developerOptions = useMemo<Option[]>(() => {
    const values = Object.entries(developerCounts)
      .filter(([, count]) => count > 0)
      .map(([value, count]) => ({
        value,
        // 空串 = 扫描出来但没有会社信息（多是刚入库还没刮削的）
        label: value || "未登记",
        count,
      }));
    return buildOptions(values, filters.developers, search);
  }, [developerCounts, filters.developers, search]);

  const engineOptions = useMemo<Option[]>(() => {
    const values = Object.keys(engineCounts).map((e) => ({
      value: e,
      label: engineLabel(e),
      count: engineCounts[e] ?? 0,
    }));
    return buildOptions(values, filters.engines, search);
  }, [engineCounts, filters.engines, search]);

  const tagOptions = useMemo<Option[]>(() => {
    const values = allTags
      .filter((t) => (tagCounts[t] ?? 0) > 0)
      .map((t) => ({ value: t, label: t, count: tagCounts[t] ?? 0 }));
    return buildOptions(values, filters.tags, search);
  }, [allTags, filters.tags, search, tagCounts]);

  const visiblePlatforms = buildOptions(platformOptions, filters.platforms, search);
  const totalShown =
    visiblePlatforms.length +
    developerOptions.length +
    engineOptions.length +
    tagOptions.length;

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-background">
      <div className="flex items-center justify-between px-4 pb-2 pt-4">
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

      <div className="px-3 pb-2">
        <div className="relative">
          <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索会社 / 引擎 / 标签"
            aria-label="搜索筛选项"
            className="h-7 rounded-md border-border bg-card pl-7 pr-7 text-[12px] placeholder:text-muted-foreground/80"
          />
          {searching && (
            <button
              type="button"
              onClick={() => setSearch("")}
              aria-label="清空筛选搜索"
              className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
            >
              <X className="size-3" />
            </button>
          )}
        </div>
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="px-1.5 pb-4">
          <FilterGroup
            title="会社"
            options={developerOptions}
            selected={filters.developers}
            onToggle={onToggleDeveloper}
            searching={searching}
          />
          <FilterGroup
            title="引擎"
            options={engineOptions}
            selected={filters.engines}
            onToggle={(v) => onToggleEngine(v as Engine)}
            searching={searching}
          />
          <FilterGroup
            title="标签"
            options={tagOptions}
            selected={filters.tags}
            onToggle={onToggleTag}
            searching={searching}
            defaultOpen={false}
          />
          <FilterGroup
            title="平台"
            options={visiblePlatforms}
            selected={filters.platforms}
            onToggle={(v) => onTogglePlatform(v as Platform)}
            searching={searching}
            defaultOpen={false}
          />

          {searching && totalShown === 0 && (
            <p className="px-2 py-6 text-center text-[12px] text-muted-foreground">
              没有匹配「{search.trim()}」的筛选项
            </p>
          )}
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
