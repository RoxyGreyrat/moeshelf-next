import { useCallback, useEffect, useState } from "react";
import {
  ArrowUp,
  FileImage,
  Folder,
  HardDrive,
  Loader2,
  Check,
} from "lucide-react";

import { listPath } from "@/lib/api";
import type { PathEntry, PathListing } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const IMAGE_EXT = /\.(jpe?g|png|webp|gif|bmp|avif)$/i;

interface PathPickerDialogProps {
  open: boolean;
  /** folder = 选目录；image = 选一张图片 */
  mode: "folder" | "image";
  title?: string;
  onOpenChange: (open: boolean) => void;
  onPick: (path: string) => void;
}

/**
 * 自制的路径选择器。
 *
 * 为什么不用系统对话框：Windows 的文件夹选择框是另一套视觉（旧式树控件），
 * 和这套亮色界面放在一起像两个软件。这里只调 Rust 的 `list_path` 拿目录内容，
 * 渲染完全自己控制。
 *
 * 交互：
 * - 单击文件夹 = 进入（比"单击选中 + 双击进入"少一步，选目录场景更顺）
 * - 选图片时单击文件 = 选中，双击 = 直接确认
 * - 底部固定显示当前路径与确认按钮
 */
export function PathPickerDialog({
  open,
  mode,
  title,
  onOpenChange,
  onPick,
}: PathPickerDialogProps) {
  const [listing, setListing] = useState<PathListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  /** 直接粘一个路径跳过去：游戏库常在 E://galgame//会社//作品 这种深目录，一层层点太慢 */
  const [jump, setJump] = useState("");

  const load = useCallback(async (path: string | null) => {
    setLoading(true);
    setError(null);
    setSelectedFile(null);
    try {
      const next = await listPath(path);
      setListing(next);
    } catch (err) {
      console.error("[list_path]", err);
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  // 每次打开都回到盘符列表，避免上次的目录被删了打不开
  useEffect(() => {
    if (!open) return;
    void load(null);
  }, [open, load]);

  const currentPath = listing?.path ?? null;

  const entries = (listing?.entries ?? []).filter((e) =>
    mode === "image" ? e.isDir || IMAGE_EXT.test(e.name) : true,
  );

  const pickCurrent = () => {
    if (mode === "folder") {
      if (currentPath) {
        onPick(currentPath);
        onOpenChange(false);
      }
      return;
    }
    if (selectedFile) {
      onPick(selectedFile);
      onOpenChange(false);
    }
  };

  const canConfirm = mode === "folder" ? !!currentPath : !!selectedFile;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-[640px]">
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="text-[15px]">
            {title ?? (mode === "folder" ? "选择文件夹" : "选择图片")}
          </DialogTitle>
          <DialogDescription className="text-[11.5px]">
            {mode === "folder"
              ? "进入目标文件夹后点右下角确认；这里不调用系统对话框。"
              : "支持 jpg / png / webp / gif / bmp。"}
          </DialogDescription>
        </DialogHeader>

        {/* 路径条 */}
        <div className="flex items-center gap-1.5 border-b border-border bg-muted/30 px-3 py-2">
          <Button
            variant="ghost"
            size="icon"
            className="size-7 shrink-0 text-muted-foreground"
            aria-label="上一级"
            disabled={!listing?.parent}
            onClick={() => load(listing?.parent ?? null)}
          >
            <ArrowUp className="size-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 shrink-0 gap-1 px-1.5 text-[11.5px] text-muted-foreground"
            onClick={() => load(null)}
          >
            <HardDrive className="size-3.5" />
            此电脑
          </Button>
          <span className="min-w-0 flex-1 truncate text-[11.5px] text-foreground" title={currentPath ?? ""}>
            {currentPath ?? "（选择磁盘）"}
          </span>
          <Input
            value={jump}
            onChange={(e) => setJump(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && jump.trim()) {
                void load(jump.trim());
                setJump("");
              }
            }}
            placeholder="粘贴路径后回车"
            className="h-7 w-[190px] shrink-0 bg-card text-[11px]"
          />
        </div>

        {/* 列表 */}
        <div className="h-[360px] min-h-0 overflow-y-auto px-2 py-1.5">
          {loading ? (
            <div className="flex h-full items-center justify-center">
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            </div>
          ) : error ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
              <p className="text-[12.5px] font-medium">打不开这个目录</p>
              <p className="text-[11px] leading-relaxed text-muted-foreground">{error}</p>
            </div>
          ) : entries.length === 0 ? (
            <p className="flex h-full items-center justify-center text-[12px] text-muted-foreground">
              这个目录里没有可选的{mode === "image" ? "图片" : "文件夹"}。
            </p>
          ) : (
            <ul className="space-y-px">
              {entries.map((e) => (
                <PathRow
                  key={e.path}
                  entry={e}
                  selected={selectedFile === e.path}
                  onEnter={() => {
                    if (e.isDir) void load(e.path);
                    else setSelectedFile(e.path);
                  }}
                  onPickFile={() => {
                    if (!e.isDir) {
                      onPick(e.path);
                      onOpenChange(false);
                    }
                  }}
                />
              ))}
            </ul>
          )}
        </div>

        {/* 底栏 */}
        <div className="flex items-center justify-between gap-3 border-t border-border px-5 py-3">
          <p className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground" title={selectedFile ?? ""}>
            {mode === "folder"
              ? currentPath ?? "还没有进入任何目录"
              : selectedFile
                ? selectedFile.split(/[\\/]/).pop()
                : "还没有选中图片"}
          </p>
          <Button variant="outline" size="sm" className="h-8" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button size="sm" className="h-8 gap-1.5" disabled={!canConfirm} onClick={pickCurrent}>
            <Check className="size-3.5" />
            {mode === "folder" ? "选择此文件夹" : "使用这张图片"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function PathRow({
  entry,
  selected,
  onEnter,
  onPickFile,
}: {
  entry: PathEntry;
  selected: boolean;
  onEnter: () => void;
  onPickFile: () => void;
}) {
  const Icon = entry.isDir ? Folder : FileImage;

  return (
    <li>
      <button
        type="button"
        onClick={onEnter}
        onDoubleClick={entry.isDir ? undefined : onPickFile}
        className={cn(
          "flex w-full items-center gap-2.5 rounded-md px-2.5 py-[7px] text-left transition-colors duration-100",
          selected ? "bg-accent text-accent-foreground" : "hover:bg-muted/60",
        )}
      >
        <Icon
          className={cn(
            "size-4 shrink-0",
            entry.isDir ? "text-muted-foreground" : "text-primary/70",
          )}
        />
        <span className="min-w-0 flex-1 truncate text-[12.5px]">{entry.name}</span>
        {entry.isDir && (
          <span className="shrink-0 text-[10.5px] text-muted-foreground/60">进入</span>
        )}
      </button>
    </li>
  );
}
