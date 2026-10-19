import { useEffect, useState } from "react";
import {
  ImageIcon,
  Images,
  Link2,
  Loader2,
  RotateCcw,
  Save,
  Upload,
} from "lucide-react";
import { toast } from "sonner";

import { resetCover, setCoverFromFile, setCoverFromUrl, updateGame } from "@/lib/api";
import type { GamePatch } from "@/lib/api";
import type { Engine, Game, Platform } from "@/types/game";
import { ENGINE_LABEL, PLATFORM_LABEL } from "@/types/game";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { PathPickerDialog } from "@/components/common/PathPickerDialog";
import { CoverPickerDialog } from "@/components/detail/CoverPickerDialog";
import { coverSrc } from "@/lib/cover";

interface GameEditDialogProps {
  open: boolean;
  game: Game;
  onOpenChange: (open: boolean) => void;
  /** 保存成功后把最新一行回传，调用方据此刷新页面 */
  onSaved: (game: Game) => void;
}

interface Draft {
  titleCn: string;
  title: string;
  originalTitle: string;
  developer: string;
  released: string;
  engine: Engine;
  platform: Platform;
  rating: string;
  tags: string;
  cnDescription: string;
}

function toDraft(g: Game): Draft {
  return {
    titleCn: g.titleCn,
    title: g.title,
    originalTitle: g.originalTitle,
    developer: g.developer,
    released: g.released,
    engine: g.engine,
    platform: g.platform,
    rating: String(g.rating || ""),
    tags: g.tags.join("、"),
    cnDescription: g.cnDescription ?? "",
  };
}

/**
 * 全屏详情页里的「编辑信息」。
 *
 * 用弹窗而不是把整页切成表单：详情页的排版（封面 + 角色 + 简介）本身就是信息，
 * 切成表单会把"我在看这部游戏"这个上下文弄丢。弹窗浮在上面，改完立刻能看到结果。
 *
 * 封面是**独立动作**，不跟表单一起提交：选完图就落库并立刻换掉预览，
 * 用户能马上判断好不好看，不满意可以再换。
 */
export function GameEditDialog({
  open,
  game,
  onOpenChange,
  onSaved,
}: GameEditDialogProps) {
  const [draft, setDraft] = useState<Draft>(() => toDraft(game));
  const [saving, setSaving] = useState(false);
  const [coverBusy, setCoverBusy] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [vndbPickerOpen, setVndbPickerOpen] = useState(false);
  const [urlInput, setUrlInput] = useState("");
  const [coverWarn, setCoverWarn] = useState(false);

  useEffect(() => {
    if (open) {
      setDraft(toDraft(game));
      setUrlInput("");
      setCoverWarn(false);
    }
  }, [open, game]);

  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));

  const applyCover = (next: Game, label: string) => {
    toast.success(label);
    onSaved(next);
    setCoverWarn(false);
  };

  const pickLocalCover = async (path: string) => {
    setCoverBusy(true);
    try {
      const next = await setCoverFromFile(game.id, path);
      applyCover(next, "封面已更新");
    } catch (err) {
      console.error("[set_cover_from_file]", err);
      toast.error("换封面失败", { description: String(err) });
    } finally {
      setCoverBusy(false);
    }
  };

  const applyUrlCover = async () => {
    const url = urlInput.trim();
    if (!url) return;
    setCoverBusy(true);
    try {
      const next = await setCoverFromUrl(game.id, url);
      applyCover(next, "封面已更新");
      setUrlInput("");
    } catch (err) {
      console.error("[set_cover_from_url]", err);
      toast.error("换封面失败", { description: String(err) });
    } finally {
      setCoverBusy(false);
    }
  };

  const restoreCover = async () => {
    setCoverBusy(true);
    try {
      const next = await resetCover(game.id);
      applyCover(next, "已恢复默认封面");
    } catch (err) {
      console.error("[reset_cover]", err);
      toast.error("恢复失败", { description: String(err) });
    } finally {
      setCoverBusy(false);
    }
  };

  const submit = async () => {
    const titleCn = draft.titleCn.trim();
    if (!titleCn) {
      toast.error("中文标题不能为空");
      return;
    }

    const rating = draft.rating.trim() === "" ? 0 : Number(draft.rating);
    if (Number.isNaN(rating) || rating < 0 || rating > 10) {
      toast.error("评分要在 0 ~ 10 之间");
      return;
    }

    // 标签分隔符只认 、 , ，—— 空格不许拆，否则 "White Haired Heroine" 会被拆碎
    const tags = draft.tags
      .split(/[、,，]+/)
      .map((t) => t.trim())
      .filter(Boolean);

    const patch: GamePatch = {};
    if (titleCn !== game.titleCn) patch.titleCn = titleCn;
    if (draft.title.trim() !== game.title) patch.title = draft.title.trim();
    if (draft.originalTitle.trim() !== game.originalTitle)
      patch.originalTitle = draft.originalTitle.trim();
    if (draft.developer.trim() !== game.developer)
      patch.developer = draft.developer.trim();
    if (draft.released.trim() !== game.released) patch.released = draft.released.trim();
    if (draft.engine !== game.engine) patch.engine = draft.engine;
    if (draft.platform !== game.platform) patch.platform = draft.platform;
    if (rating !== game.rating) patch.rating = rating;
    if (tags.join("|") !== game.tags.join("|")) patch.tags = tags;
    if (draft.cnDescription !== (game.cnDescription ?? ""))
      patch.cnDescription = draft.cnDescription;
    else if (draft.cnDescription !== "" && !game.cnDescription)
      patch.description = draft.cnDescription;

    if (Object.keys(patch).length === 0) {
      toast("没有检测到改动");
      onOpenChange(false);
      return;
    }

    setSaving(true);
    try {
      const next = await updateGame(game.id, patch);
      onSaved(next);
      onOpenChange(false);
      toast.success(`已保存 ${Object.keys(patch).length} 个字段`, {
        description: "这些字段已锁定，重新扫描和获取资料都不会覆盖。",
      });
    } catch (err) {
      console.error("[update_game]", err);
      toast.error("保存失败", { description: String(err) });
    } finally {
      setSaving(false);
    }
  };

  const cover = coverSrc(game.coverUrl);

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="gap-0 p-0 sm:max-w-[760px]">
          <DialogHeader className="border-b border-border px-5 py-4">
            <DialogTitle className="text-[15px]">编辑信息</DialogTitle>
            <DialogDescription className="text-[11.5px]">
              改完的字段会被锁定，之后重新扫描或获取资料都不会覆盖。
            </DialogDescription>
          </DialogHeader>

          <div className="max-h-[70vh] overflow-y-auto px-5 py-4">
            <div className="flex gap-5">
              {/* 封面 */}
              <div className="w-[132px] shrink-0">
                <Label className="text-[11px] text-muted-foreground">封面</Label>
                <div className="relative mt-1.5 aspect-[2/3] overflow-hidden rounded-lg bg-muted ring-1 ring-black/5">
                  {cover && !coverWarn ? (
                    <img
                      src={cover}
                      alt="封面"
                      className="h-full w-full object-cover"
                      onError={() => setCoverWarn(true)}
                    />
                  ) : (
                    <div className="flex h-full w-full flex-col items-center justify-center gap-1 text-muted-foreground">
                      <ImageIcon className="size-5" />
                      <span className="text-[10px]">暂无封面</span>
                    </div>
                  )}
                  {coverBusy && (
                    <div className="absolute inset-0 flex items-center justify-center bg-background/70">
                      <Loader2 className="size-4 animate-spin text-muted-foreground" />
                    </div>
                  )}
                </div>

                <div className="mt-2 space-y-1.5">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 w-full gap-1 px-2 text-[11px]"
                    disabled={coverBusy}
                    onClick={() => setPickerOpen(true)}
                  >
                    <Upload className="size-3" />
                    本地图片
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 w-full gap-1 px-2 text-[11px]"
                    disabled={coverBusy}
                    onClick={() => setVndbPickerOpen(true)}
                  >
                    <Images className="size-3" />
                    从 VNDB 选
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 w-full gap-1 px-2 text-[11px] text-muted-foreground"
                    disabled={coverBusy}
                    onClick={restoreCover}
                  >
                    <RotateCcw className="size-3" />
                    恢复默认
                  </Button>
                </div>
              </div>

              {/* 字段 */}
              <div className="min-w-0 flex-1 space-y-3">
                <div>
                  <Label className="text-[11px] text-muted-foreground">中文标题</Label>
                  <Input
                    value={draft.titleCn}
                    onChange={(e) => set("titleCn", e.target.value)}
                    className="mt-1 h-8 text-[12.5px]"
                  />
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <Label className="text-[11px] text-muted-foreground">原名</Label>
                    <Input
                      value={draft.originalTitle}
                      onChange={(e) => set("originalTitle", e.target.value)}
                      className="mt-1 h-8 text-[12.5px]"
                    />
                  </div>
                  <div>
                    <Label className="text-[11px] text-muted-foreground">罗马字 / 英文</Label>
                    <Input
                      value={draft.title}
                      onChange={(e) => set("title", e.target.value)}
                      className="mt-1 h-8 text-[12.5px]"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <Label className="text-[11px] text-muted-foreground">开发商</Label>
                    <Input
                      value={draft.developer}
                      onChange={(e) => set("developer", e.target.value)}
                      className="mt-1 h-8 text-[12.5px]"
                    />
                  </div>
                  <div>
                    <Label className="text-[11px] text-muted-foreground">发售日</Label>
                    <Input
                      value={draft.released}
                      onChange={(e) => set("released", e.target.value)}
                      placeholder="YYYY-MM-DD"
                      className="mt-1 h-8 text-[12.5px]"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-3 gap-3">
                  <div>
                    <Label className="text-[11px] text-muted-foreground">引擎</Label>
                    <Select
                      value={draft.engine}
                      onValueChange={(v) => set("engine", v as Engine)}
                    >
                      <SelectTrigger className="mt-1 h-8 text-[12px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {(Object.keys(ENGINE_LABEL) as Engine[]).map((e) => (
                          <SelectItem key={e} value={e} className="text-[12px]">
                            {ENGINE_LABEL[e]}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                  <div>
                    <Label className="text-[11px] text-muted-foreground">平台</Label>
                    <Select
                      value={draft.platform}
                      onValueChange={(v) => set("platform", v as Platform)}
                    >
                      <SelectTrigger className="mt-1 h-8 text-[12px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {(Object.keys(PLATFORM_LABEL) as Platform[]).map((p) => (
                          <SelectItem key={p} value={p} className="text-[12px]">
                            {PLATFORM_LABEL[p]}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                  <div>
                    <Label className="text-[11px] text-muted-foreground">评分 0~10</Label>
                    <Input
                      value={draft.rating}
                      onChange={(e) => set("rating", e.target.value)}
                      inputMode="decimal"
                      className="mt-1 h-8 text-[12.5px]"
                    />
                  </div>
                </div>

                <div>
                  <Label className="text-[11px] text-muted-foreground">
                    标签（用 、 分隔，标签内的空格会保留）
                  </Label>
                  <Input
                    value={draft.tags}
                    onChange={(e) => set("tags", e.target.value)}
                    className="mt-1 h-8 text-[12.5px]"
                  />
                </div>

                <div>
                  <Label className="text-[11px] text-muted-foreground">中文简介</Label>
                  <Textarea
                    value={draft.cnDescription}
                    onChange={(e) => set("cnDescription", e.target.value)}
                    rows={4}
                    className="mt-1 min-h-[88px] text-[12px] leading-relaxed"
                  />
                </div>
              </div>
            </div>

            {/* 封面上传：URL */}
            <div className="mt-4 flex items-center gap-2 rounded-lg border border-border bg-muted/30 p-2">
              <Link2 className="ml-1 size-3.5 shrink-0 text-muted-foreground" />
              <Input
                value={urlInput}
                onChange={(e) => setUrlInput(e.target.value)}
                placeholder="粘贴图片地址（会下载到本地，断网也不影响）"
                className="h-7 flex-1 border-0 bg-transparent text-[11.5px] shadow-none focus-visible:ring-0"
                onKeyDown={(e) => {
                  if (e.key === "Enter") void applyUrlCover();
                }}
              />
              <Button
                size="sm"
                variant="outline"
                className="h-7 shrink-0 px-2 text-[11px]"
                disabled={coverBusy || !urlInput.trim()}
                onClick={applyUrlCover}
              >
                使用
              </Button>
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
            <Button variant="outline" size="sm" className="h-8" onClick={() => onOpenChange(false)}>
              取消
            </Button>
            <Button size="sm" className="h-8 gap-1.5" disabled={saving} onClick={submit}>
              {saving ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Save className="size-3.5" />
              )}
              {saving ? "保存中…" : "保存并锁定"}
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      <PathPickerDialog
        open={pickerOpen}
        mode="image"
        title="选择封面图片"
        onOpenChange={setPickerOpen}
        onPick={(path) => void pickLocalCover(path)}
      />

      <CoverPickerDialog
        open={vndbPickerOpen}
        game={game}
        onOpenChange={setVndbPickerOpen}
        onPicked={(next) => {
          onSaved(next);
          setCoverWarn(false);
        }}
      />
    </>
  );
}
