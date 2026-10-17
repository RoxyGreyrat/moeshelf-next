import { Loader2, Trash2 } from "lucide-react";

import type { Game } from "@/types/game";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface DeleteGamesDialogProps {
  open: boolean;
  /** 待删除的条目：单选时 1 条，多选时是选中的全部 */
  games: Game[];
  /** 删除请求正在飞（按钮转圈 + 禁止关闭，避免删一半） */
  busy: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}

/**
 * 删除确认。
 *
 * 文案里必须讲清两件事，否则用户会以为是在删硬盘上的游戏：
 * 1. 只删数据库记录，不动文件；
 * 2. 再扫描一次这个目录，它会作为"新游戏"回来。
 * 只是不想看见它 → 应该用「忽略」。
 */
export function DeleteGamesDialog({
  open,
  games,
  busy,
  onOpenChange,
  onConfirm,
}: DeleteGamesDialogProps) {
  const count = games.length;
  const preview = games.slice(0, 4);

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next);
      }}
    >
      <DialogContent className="sm:max-w-[440px]">
        <DialogHeader>
          <DialogTitle>
            从书架删除{count > 1 ? `这 ${count} 部` : "这一部"}？
          </DialogTitle>
          <DialogDescription className="leading-relaxed">
            只删数据库里的记录，<strong className="text-foreground">不会动硬盘上的游戏文件</strong>，
            已缓存的封面也留着。下次扫描这个目录，它会作为新游戏重新出现。
            <br />
            如果只是不想在书架上看见它，用「忽略」更合适。
          </DialogDescription>
        </DialogHeader>

        <ul className="space-y-1 rounded-lg bg-muted/60 px-3 py-2">
          {preview.map((g) => (
            <li
              key={g.id}
              className="truncate text-[12px] text-foreground"
              title={g.folderPath}
            >
              {g.titleCn || g.folderPath}
            </li>
          ))}
          {count > preview.length && (
            <li className="text-[11.5px] text-muted-foreground">
              还有 {count - preview.length} 部…
            </li>
          )}
        </ul>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            取消
          </Button>
          <Button variant="destructive" onClick={onConfirm} disabled={busy} className="gap-1.5">
            {busy ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Trash2 className="size-4" />
            )}
            {busy ? "删除中…" : "删除"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
