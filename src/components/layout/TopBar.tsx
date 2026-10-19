import {
  ArrowDownWideNarrow,
  ArrowUpDown,
  ArrowUpNarrowWide,
  Check,
  ChevronDown,
  FolderOpen,
  FolderPlus,
  LayoutGrid,
  List,
  Loader2,
  ScanLine,
  Search,
  Settings,
  X,
} from "lucide-react";
import type { SortKey, SortState, ViewMode } from "@/types/game";
import type { ScanRoot } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface TopBarProps {
  query: string;
  onQueryChange: (v: string) => void;
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortState;
  onSortChange: (s: SortState) => void;
  /** 传了 root 就直接扫这个目录；不传则弹文件夹选择框 */
  onScan: (root?: string) => void;
  scanning: boolean;
  /** 已登记的扫描根目录；为空时扫描按钮退化成普通的"选目录" */
  roots: ScanRoot[];
  onOpenSettings: () => void;
  searchRef: React.RefObject<HTMLInputElement | null>;
}

/** 排序字段。顺序就是下拉里的顺序：玩家最常用的是发售日和名称 */
const SORT_OPTIONS: { key: SortKey; label: string }[] = [
  { key: "released", label: "发售日" },
  { key: "title", label: "名称" },
  { key: "rating", label: "评分" },
  { key: "added", label: "入库时间" },
];

/** `E:\galgame\yuzusoft` → `yuzusoft`；`E:\` → `E:\` */
function baseName(path: string): string {  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= 1) return path;
  return parts[parts.length - 1];
}

function shortDate(iso: string | null): string {
  if (!iso) return "从未扫描";
  return iso.slice(0, 10);
}

export function TopBar({
  query,
  onQueryChange,
  view,
  onViewChange,
  sort,
  onSortChange,
  onScan,
  scanning,
  roots,
  onOpenSettings,
  searchRef,
}: TopBarProps) {
  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur-sm">
      <div className="relative w-full max-w-[340px]">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          ref={searchRef}
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          placeholder="搜索标题、会社或标签…"
          aria-label="搜索游戏"
          className="h-9 rounded-lg border-border bg-card pl-9 pr-16 text-[13px] placeholder:text-muted-foreground"
        />
        {query ? (
          <button
            type="button"
            onClick={() => onQueryChange("")}
            aria-label="清空搜索"
            className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        ) : (
          <kbd className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 rounded border border-border bg-muted px-1.5 py-0.5 font-sans text-[10px] text-muted-foreground">
            Ctrl K
          </kbd>
        )}
      </div>

      <div className="ml-auto flex items-center gap-2">
        {/* 排序：一个下拉选字段 + 一个按钮翻方向。
            拆成两个控件而不是「发售日↑/发售日↓」八项下拉：切字段时方向不会被重置。 */}
        <div className="flex items-center gap-0.5 rounded-lg border border-border bg-card p-0.5">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="h-7 gap-1 rounded-md px-2 text-[12px] font-normal text-foreground"
                aria-label="排序方式"
              >
                <ArrowUpDown className="size-3.5 text-muted-foreground" />
                {SORT_OPTIONS.find((o) => o.key === sort.key)?.label ?? "排序"}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-[150px]">
              <DropdownMenuLabel className="text-[10.5px] font-normal tracking-wide text-muted-foreground">
                排序方式
              </DropdownMenuLabel>
              {SORT_OPTIONS.map((o) => (
                <DropdownMenuItem
                  key={o.key}
                  onSelect={() => onSortChange({ ...sort, key: o.key })}
                  className="gap-2 text-[12.5px]"
                >
                  <span className="flex-1">{o.label}</span>
                  {sort.key === o.key && <Check className="size-3.5" />}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>

          <button
            type="button"
            onClick={() =>
              onSortChange({ ...sort, dir: sort.dir === "asc" ? "desc" : "asc" })
            }
            aria-label={sort.dir === "desc" ? "当前倒序，点击改为正序" : "当前正序，点击改为倒序"}
            title={sort.dir === "desc" ? "倒序（点击改成正序）" : "正序（点击改成倒序）"}
            className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            {sort.dir === "desc" ? (
              <ArrowDownWideNarrow className="size-3.5" />
            ) : (
              <ArrowUpNarrowWide className="size-3.5" />
            )}
          </button>
        </div>

        <ToggleGroup
          type="single"
          value={view}
          onValueChange={(v) => v && onViewChange(v as ViewMode)}
          className="gap-0.5 rounded-lg border border-border bg-card p-0.5"
          aria-label="视图切换"
        >
          <ToggleGroupItem
            value="grid"
            aria-label="网格视图"
            className={cn(
              "h-7 w-8 rounded-md data-[state=on]:bg-accent data-[state=on]:text-accent-foreground",
            )}
          >
            <LayoutGrid className="size-4" />
          </ToggleGroupItem>
          <ToggleGroupItem
            value="list"
            aria-label="列表视图"
            className="h-7 w-8 rounded-md data-[state=on]:bg-accent data-[state=on]:text-accent-foreground"
          >
            <List className="size-4" />
          </ToggleGroupItem>
        </ToggleGroup>

        {roots.length === 0 ? (
          <Button
            onClick={() => onScan()}
            size="sm"
            disabled={scanning}
            className="h-9 gap-1.5 px-3"
          >
            {scanning ? (
              <>
                <Loader2 className="size-4 animate-spin" />
                扫描中…
              </>
            ) : (
              <>
                <ScanLine className="size-4" />
                扫描文件夹
              </>
            )}
          </Button>
        ) : (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="sm" disabled={scanning} className="h-9 gap-1.5 px-3">
                {scanning ? (
                  <>
                    <Loader2 className="size-4 animate-spin" />
                    扫描中…
                  </>
                ) : (
                  <>
                    <ScanLine className="size-4" />
                    扫描
                    <ChevronDown className="size-3.5 opacity-70" />
                  </>
                )}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-[300px]">
              <DropdownMenuLabel className="text-[10.5px] font-normal tracking-wide text-muted-foreground">
                已登记的目录
              </DropdownMenuLabel>
              {roots.map((r) => (
                <DropdownMenuItem
                  key={r.id}
                  onSelect={() => onScan(r.path)}
                  className="gap-2"
                >
                  <FolderOpen className="size-3.5 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[12.5px] font-medium">
                      {baseName(r.path)}
                    </p>
                    <p
                      className="truncate text-[10.5px] text-muted-foreground"
                      title={r.path}
                    >
                      {r.path}
                    </p>
                  </div>
                  <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                    {shortDate(r.lastScanAt)}
                  </span>
                </DropdownMenuItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={() => onScan()} className="gap-2">
                <FolderPlus className="size-3.5 text-muted-foreground" />
                <span className="text-[12.5px]">选择其他目录…</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}

        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              onClick={onOpenSettings}
              className="size-9 text-muted-foreground hover:text-foreground"
              aria-label="设置"
            >
              <Settings className="size-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">设置</TooltipContent>
        </Tooltip>
      </div>
    </header>
  );
}
