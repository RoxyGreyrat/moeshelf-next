import { useEffect, useState } from "react";
import { Loader2, Save } from "lucide-react";
import { toast } from "sonner";

import { updateGame } from "@/lib/api";
import type { GamePatch } from "@/lib/api";
import type { Game } from "@/types/game";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

interface DescriptionEditDialogProps {
  open: boolean;
  game: Game;
  onOpenChange: (open: boolean) => void;
  onSaved: (game: Game) => void;
}

/**
 * 详情页「简介」区块右上角的小编辑器。
 *
 * 简介是**单个字段的编辑**，塞进「编辑信息」那个大表单里会让两件事互相拖累：
 * 用户只想补一段中文简介，却要面对标题 / 会社 / 评分 / 标签一屏表单，
 * 还得担心误改别的字段。所以单独一个轻量弹窗。
 *
 * 中英文分开两个框：Rust 侧本来就分 `cn_description` / `description` 两列，
 * 合在一个框里会分不清该写哪一列（老版本就是这个问题）。
 */
export function DescriptionEditDialog({
  open,
  game,
  onOpenChange,
  onSaved,
}: DescriptionEditDialogProps) {
  const [cn, setCn] = useState("");
  const [foreign, setForeign] = useState("");
  const [saving, setSaving] = useState(false);

  // 每次打开都从当前数据重建草稿：关掉再开不能留上一次的残留
  useEffect(() => {
    if (!open) return;
    setCn(game.cnDescription ?? "");
    setForeign(game.description ?? "");
  }, [open, game.id, game.cnDescription, game.description]);

  const save = async () => {
    const patch: GamePatch = {};
    const nextCn = cn.trim();
    const nextForeign = foreign.trim();

    if (nextCn !== (game.cnDescription ?? "")) patch.cnDescription = nextCn;
    if (nextForeign !== game.description) patch.description = nextForeign;

    if (Object.keys(patch).length === 0) {
      onOpenChange(false);
      return;
    }

    setSaving(true);
    try {
      const updated = await updateGame(game.id, patch);
      onSaved(updated);
      onOpenChange(false);
      toast.success("简介已保存", {
        description: "这两个字段已锁定，重新获取资料不会覆盖。",
      });
    } catch (err) {
      console.error("[update_game]", err);
      toast.error("保存失败", { description: String(err) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-[640px]">
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="text-[15px]">编辑简介</DialogTitle>
          <DialogDescription className="text-[11.5px] leading-relaxed">
            详情页优先显示中文简介；中文为空时才显示外文简介。
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 px-5 py-4">
          <div>
            <Label className="text-[11px] text-muted-foreground">中文简介</Label>
            <Textarea
              value={cn}
              onChange={(e) => setCn(e.target.value)}
              rows={6}
              placeholder="留空则详情页显示外文简介"
              className="mt-1 min-h-[132px] text-[12px] leading-relaxed"
            />
          </div>

          <div>
            <Label className="text-[11px] text-muted-foreground">外文简介</Label>
            <Textarea
              value={foreign}
              onChange={(e) => setForeign(e.target.value)}
              rows={5}
              placeholder="VNDB 提供的英文简介会存在这里"
              className="mt-1 min-h-[110px] text-[12px] leading-relaxed"
            />
          </div>
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-border px-5 py-3">
          <p className="text-[11px] text-muted-foreground">保存后这两个字段会被锁定</p>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-8"
              onClick={() => onOpenChange(false)}
            >
              取消
            </Button>
            <Button size="sm" className="h-8 gap-1.5" disabled={saving} onClick={() => void save()}>
              {saving ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
              {saving ? "保存中…" : "保存"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
