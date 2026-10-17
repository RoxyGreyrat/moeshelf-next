import { useEffect, useState } from "react";
import { Loader2, RotateCcw } from "lucide-react";
import { toast } from "sonner";

import { listIgnoredGames, setGamesIgnored } from "@/lib/api";
import type { Game } from "@/types/game";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 恢复了某一条之后通知上层重拉列表 */
  onRestored: () => void;
}

/**
 * 设置面板。
 *
 * 现在只有「已忽略」这一块 —— 忽略是隐藏而不是删除，所以必须有一个地方能看见、能恢复，
 * 否则用户手一抖就再也找不回来了。
 * 扫描根目录、刮削源这些等 D 阶段再往这里加。
 */
export function SettingsDialog({ open, onOpenChange, onRestored }: SettingsDialogProps) {
  const [ignored, setIgnored] = useState<Game[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);

  // 每次打开都重新读：忽略可能是在别的地方（扫描后清理）刚发生的
  useEffect(() => {
    if (!open) return;
    let alive = true;
    setLoading(true);
    listIgnoredGames()
      .then((rows) => {
        if (alive) setIgnored(rows);
      })
      .catch((err) => {
        console.error("[list_ignored_games]", err);
        toast.error("读取忽略清单失败", { description: String(err) });
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [open]);

  const restore = async (g: Game) => {
    setBusyId(g.id);
    try {
      await setGamesIgnored([g.id], false);
      setIgnored((prev) => prev.filter((x) => x.id !== g.id));
      onRestored();
      toast.success(`已恢复《${g.titleCn || g.folderPath}》`);
    } catch (err) {
      console.error("[set_games_ignored]", err);
      toast.error("恢复失败", { description: String(err) });
    } finally {
      setBusyId(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>设置</DialogTitle>
          <DialogDescription>
            被忽略的条目不会出现在书架里，重新扫描同一个目录也不会再冒出来。恢复后会立刻回到书架。
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-2">
          <div className="flex items-baseline justify-between">
            <h3 className="text-[12px] font-medium text-foreground">已忽略</h3>
            <span className="text-[11px] tabular-nums text-muted-foreground">
              {loading ? "读取中…" : `${ignored.length} 条`}
            </span>
          </div>

          {loading ? (
            <div className="flex h-24 items-center justify-center">
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            </div>
          ) : ignored.length === 0 ? (
            <p className="rounded-lg border border-dashed border-border px-3 py-6 text-center text-[12px] text-muted-foreground">
              没有被忽略的条目。
              <br />
              在详情面板下方点「忽略」，把工具目录这类误识别的东西藏起来。
            </p>
          ) : (
            <ScrollArea className="max-h-[300px] rounded-lg border border-border">
              <ul className="divide-y divide-border">
                {ignored.map((g) => (
                  <li key={g.id} className="flex items-center gap-3 px-3 py-2">
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[12.5px] font-medium text-foreground">
                        {g.titleCn || g.folderPath}
                      </p>
                      <p
                        className="truncate text-[11px] text-muted-foreground"
                        title={g.folderPath}
                      >
                        {g.folderPath}
                      </p>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => restore(g)}
                      disabled={busyId === g.id}
                      className="h-7 shrink-0 gap-1 px-2 text-[11.5px]"
                    >
                      {busyId === g.id ? (
                        <Loader2 className="size-3.5 animate-spin" />
                      ) : (
                        <RotateCcw className="size-3.5" />
                      )}
                      恢复
                    </Button>
                  </li>
                ))}
              </ul>
            </ScrollArea>
          )}

          <p className="pt-1 text-[11px] text-muted-foreground/80">
            扫描根目录、刮削源等设置会在后续阶段接入。
          </p>
        </div>
      </DialogContent>
    </Dialog>
  );
}
