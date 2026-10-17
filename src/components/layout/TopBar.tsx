import { LayoutGrid, List, ScanLine, Search, Settings, X } from "lucide-react";
import type { ViewMode } from "@/types/game";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
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
  onScan: () => void;
  onOpenSettings: () => void;
  searchRef: React.RefObject<HTMLInputElement | null>;
}

export function TopBar({
  query,
  onQueryChange,
  view,
  onViewChange,
  onScan,
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

        <Button onClick={onScan} size="sm" className="h-9 gap-1.5 px-3">
          <ScanLine className="size-4" />
          扫描文件夹
        </Button>

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
