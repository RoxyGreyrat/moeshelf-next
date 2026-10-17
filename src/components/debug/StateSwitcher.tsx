import { useState } from "react";
import { Bug, ChevronDown, DatabaseZap, Loader2 } from "lucide-react";
import type { GridState, SelectMode } from "@/types/game";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface StateSwitcherProps {
  gridState: GridState;
  onGridStateChange: (s: GridState) => void;
  selectMode: SelectMode;
  onSelectModeChange: (m: SelectMode) => void;
  editing: boolean;
  onEditingChange: (v: boolean) => void;
  onToastSuccess: () => void;
  onToastError: () => void;
  /** 触发旧库迁移（B2 临时入口，接完真实数据后删掉） */
  onMigrate: () => void;
  migrating: boolean;
}

function Seg({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "flex-1 rounded-md px-1.5 py-1 text-[11px] transition-colors duration-150",
        active
          ? "bg-primary text-primary-foreground"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <p className="text-[10px] font-medium tracking-[0.06em] text-muted-foreground">
        {label}
      </p>
      <div className="flex gap-1 rounded-lg border border-border bg-background p-0.5">
        {children}
      </div>
    </div>
  );
}

export function StateSwitcher({
  gridState,
  onGridStateChange,
  selectMode,
  onSelectModeChange,
  editing,
  onEditingChange,
  onToastSuccess,
  onToastError,
  onMigrate,
  migrating,
}: StateSwitcherProps) {
  const [open, setOpen] = useState(true);

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-label="展开状态调试器"
        className="fixed bottom-4 left-[15.5rem] z-40 flex size-9 items-center justify-center rounded-full border border-border bg-card text-muted-foreground shadow-[0_8px_24px_-10px_rgba(24,24,27,0.4)] transition-colors hover:text-foreground"
      >
        <Bug className="size-4" />
      </button>
    );
  }

  return (
    <div className="fixed bottom-4 left-[15.5rem] z-40 w-52 space-y-2 rounded-xl border border-border bg-card p-2 shadow-[0_16px_40px_-16px_rgba(24,24,27,0.35)]">
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-[11px] font-semibold text-foreground">
          <Bug className="size-3.5 text-primary" />
          状态调试
        </span>
        <button
          type="button"
          onClick={() => setOpen(false)}
          aria-label="收起状态调试器"
          className="rounded p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          <ChevronDown className="size-3.5" />
        </button>
      </div>

      <Row label="网格">
        <Seg active={gridState === "normal"} onClick={() => onGridStateChange("normal")}>
          正常
        </Seg>
        <Seg active={gridState === "loading"} onClick={() => onGridStateChange("loading")}>
          加载
        </Seg>
        <Seg active={gridState === "empty"} onClick={() => onGridStateChange("empty")}>
          空
        </Seg>
        <Seg active={gridState === "error"} onClick={() => onGridStateChange("error")}>
          错误
        </Seg>
      </Row>

      <Row label="选择">
        <Seg active={selectMode === "single"} onClick={() => onSelectModeChange("single")}>
          单选
        </Seg>
        <Seg active={selectMode === "multi"} onClick={() => onSelectModeChange("multi")}>
          多选
        </Seg>
      </Row>

      <Row label="详情面板">
        <Seg active={!editing} onClick={() => onEditingChange(false)}>
          只读
        </Seg>
        <Seg active={editing} onClick={() => onEditingChange(true)}>
          编辑
        </Seg>
      </Row>

      <div className="space-y-1">
        <p className="text-[10px] font-medium tracking-[0.06em] text-muted-foreground">
          按钮状态
        </p>
        <div className="flex items-center gap-1.5">
          <Button size="sm" className="h-7 flex-1 px-2 text-[11px]">
            默认
          </Button>
          <Button size="sm" disabled className="h-7 flex-1 px-2 text-[11px]">
            禁用
          </Button>
          <Button size="sm" disabled className="h-7 flex-1 gap-1 px-2 text-[11px]">
            <Loader2 className="size-3 animate-spin" />
            加载
          </Button>
        </div>
      </div>

      <div className="space-y-1">
        <p className="text-[10px] font-medium tracking-[0.06em] text-muted-foreground">
          数据
        </p>
        <Button
          size="sm"
          variant="outline"
          disabled={migrating}
          onClick={onMigrate}
          className="h-7 w-full gap-1 px-2 text-[11px]"
        >
          {migrating ? (
            <Loader2 className="size-3 animate-spin" />
          ) : (
            <DatabaseZap className="size-3" />
          )}
          {migrating ? "迁移中…" : "迁移旧库（一次性）"}
        </Button>
      </div>

      <div className="space-y-1">
        <p className="text-[10px] font-medium tracking-[0.06em] text-muted-foreground">
          Toast
        </p>
        <div className="flex gap-1.5">
          <Button
            size="sm"
            variant="outline"
            onClick={onToastSuccess}
            className="h-7 flex-1 px-2 text-[11px]"
          >
            成功
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={onToastError}
            className="h-7 flex-1 px-2 text-[11px]"
          >
            失败
          </Button>
        </div>
      </div>
    </div>
  );
}
