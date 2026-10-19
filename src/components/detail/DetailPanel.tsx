import { useEffect, useState } from "react";
import { motion } from "motion/react";
import {
  EyeOff,
  FolderOpen,
  Loader2,
  Lock,
  MousePointerClick,
  Pencil,
  Play,
  Puzzle,
  Save,
  Star,
  Trash2,
  X,
} from "lucide-react";
import type { Engine, Game, Platform } from "@/types/game";
import { ENGINE_LABEL, engineLabel, PLATFORM_LABEL } from "@/types/game";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { formatBytes, formatCount } from "@/lib/format";
import { CoverArt } from "@/components/game/CoverArt";
import { LaunchControl } from "@/components/launch/LaunchControl";

interface DetailPanelProps {
  game: Game | null;
  multiCount: number;
  editing: boolean;
  /** 保存请求正在飞（按钮转圈 + 禁用，防止连点写出两次） */
  saving: boolean;
  onStartEdit: () => void;
  onCancelEdit: () => void;
  onSave: (patch: Partial<Game>) => void;
  /** 启动项确认后拿到的更新行（App 用它替换本地那一条） */
  onGameUpdated: (g: Game) => void;
  onPatch: () => void;
  /** 忽略：只是从书架上藏起来，设置里能恢复 */
  onIgnore: () => void;
  /** 删除：二次确认后从库里删掉记录 */
  onDelete: () => void;
  /** 在资源管理器里打开这部游戏的目录 */
  onOpenFolder: () => void;
}

function FieldRow({
  label,
  value,
  locked,
  mono,
}: {
  label: string;
  value: React.ReactNode;
  locked?: boolean;
  mono?: boolean;
}) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-[5px]">
      <span className="flex shrink-0 items-center gap-1 text-[12px] text-muted-foreground">
        {label}
        {locked && (
          <Lock className="size-3 text-primary/70" aria-label="该字段已锁定" />
        )}
      </span>
      <span
        className={cn(
          "selectable truncate text-right text-[13px] font-medium text-foreground",
          mono && "font-mono text-[11.5px] font-normal",
        )}
        title={typeof value === "string" ? value : undefined}
      >
        {value}
      </span>
    </div>
  );
}

function EditField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <Label className="text-[11.5px] font-medium text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  );
}

/** 编辑态草稿：只放可改字段 */
interface Draft {
  titleCn: string;
  developer: string;
  released: string;
  engine: Engine;
  platform: Platform;
  rating: string;
  tags: string;
  description: string;
}

function toDraft(g: Game): Draft {
  return {
    titleCn: g.titleCn,
    developer: g.developer,
    released: g.released,
    engine: g.engine,
    platform: g.platform,
    rating: String(g.rating),
    tags: g.tags.join("、"),
    // 编辑框里编辑的就是"用户能看到的简介"，中文优先
    description: g.cnDescription?.trim() || g.description,
  };
}

export function DetailPanel({
  game,
  multiCount,
  editing,
  saving,
  onStartEdit,
  onCancelEdit,
  onSave,
  onGameUpdated,
  onPatch,
  onIgnore,
  onDelete,
  onOpenFolder,
}: DetailPanelProps) {
  const [draft, setDraft] = useState<Draft | null>(null);
  // 长简介 / 多标签的展开状态，换一部游戏就收起
  const [descExpanded, setDescExpanded] = useState(false);
  const [tagsExpanded, setTagsExpanded] = useState(false);

  // 切换游戏 / 进入编辑态时重置草稿
  useEffect(() => {
    if (editing && game) setDraft(toDraft(game));
    else setDraft(null);
    setDescExpanded(false);
    setTagsExpanded(false);
  }, [editing, game]);

  if (!game) {
    return (
      <aside className="flex w-80 shrink-0 flex-col items-center justify-center border-l border-border bg-card px-6 text-center">
        <div className="flex size-12 items-center justify-center rounded-xl border border-border bg-background">
          <MousePointerClick className="size-5 text-muted-foreground" />
        </div>
        <p className="mt-3 text-[13px] font-medium text-foreground">未选择游戏</p>
        <p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
          点一张封面查看详情。
          <br />
          方向键移动选中，Enter 打开，Esc 关闭。
        </p>
      </aside>
    );
  }

  const locked = (key: string) => game.lockedFields.includes(key);

  // 真实库里标签常有 8~16 个（VNDB 的 genres），全量渲染会把面板顶下去；
  // 简介同样可能几百字。两个都做"先折叠、可展开"。
  const TAGS_PREVIEW = 6;
  const visibleTags = tagsExpanded ? game.tags : game.tags.slice(0, TAGS_PREVIEW);
  const hiddenTagCount = game.tags.length - visibleTags.length;

  const description = (game.cnDescription?.trim() || game.description || "").trim();
  const descIsLong = description.length > 180;
  const multi = multiCount > 1;

  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((d) => (d ? { ...d, [key]: value } : d));

  const submit = () => {
    if (!draft) return;
    const patch: Partial<Game> = {};
    if (draft.titleCn.trim() && draft.titleCn !== game.titleCn)
      patch.titleCn = draft.titleCn.trim();
    if (draft.developer.trim() && draft.developer !== game.developer)
      patch.developer = draft.developer.trim();
    if (/^\d{4}-\d{2}-\d{2}$/.test(draft.released) && draft.released !== game.released)
      patch.released = draft.released;
    if (draft.engine !== game.engine) patch.engine = draft.engine;
    if (draft.platform !== game.platform) patch.platform = draft.platform;

    const rating = Number(draft.rating);
    if (!Number.isNaN(rating) && rating >= 0 && rating <= 10 && rating !== game.rating)
      patch.rating = rating;

    // 标签：**先比原始字符串，再解析**。
    //
    // 两个坑都在这里：
    // 1. 不能按空白拆 —— VNDB 的标签大量是英文多词短语（"White Haired Heroine"），
    //    按空格拆会把一个标签拆成三个单词写回库里，属于不可逆的数据损坏。
    // 2. 必须先判断"用户到底改没改"。只要走了"解析后再比较"，任何解析差异
    //    都会被当成改动写回库 —— 上面那个坑就是这么被触发出来的。
    if (draft.tags !== game.tags.join("、")) {
      patch.tags = draft.tags
        .split(/[、,，]+/)
        .map((t) => t.trim())
        .filter(Boolean);
    }

    // 简介写回原来的位置：本来有中文简介就更新中文，否则更新英文字段
    const sourceCn = game.cnDescription?.trim();
    const nextDesc = draft.description.trim();
    if (sourceCn) {
      if (nextDesc !== sourceCn) patch.cnDescription = nextDesc;
    } else if (nextDesc !== game.description) {
      patch.description = nextDesc;
    }

    onSave(patch);
  };

  return (
    <aside className="flex w-80 shrink-0 flex-col border-l border-border bg-card">
      {/* 切换游戏时整块淡入（key 变了就重挂载）。
          刻意**不做 exit 等待**：详情面板跟着选中项高频变化，
          「先淡出再淡入」在连续按方向键时会出现空白帧，反而显得卡。 */}
      <motion.div
        key={game.id}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
        className="flex min-h-0 flex-1 flex-col"
      >
        {/* 顶部横版大图 */}
        <div className="relative h-32 shrink-0 overflow-hidden">
        <CoverArt game={game} variant="wide" />
        <div className="absolute inset-0 bg-gradient-to-t from-card via-card/25 to-transparent" />
        <div className="absolute inset-x-3 bottom-2 flex items-center gap-1.5">
          <span className="rounded-full bg-black/35 px-2 py-0.5 text-[10px] font-medium text-white backdrop-blur-sm">
            {engineLabel(game.engine)}
          </span>
          <span className="rounded-full bg-black/35 px-2 py-0.5 text-[10px] font-medium text-white backdrop-blur-sm">
            {PLATFORM_LABEL[game.platform]}
          </span>
          {game.completed && (
            <span className="rounded-full bg-primary px-2 py-0.5 text-[10px] font-medium text-primary-foreground">
              已通关
            </span>
          )}
        </div>
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="px-4 pb-4">
          {multi && (
            <div className="mb-3 rounded-lg bg-accent px-2.5 py-1.5 text-[12px] text-accent-foreground">
              已选 {multiCount} 部游戏 · 批量操作暂未开放
            </div>
          )}

          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <div className="flex items-center gap-1.5">
                <h2 className="selectable truncate text-[17px] font-semibold leading-tight tracking-tight">
                  {game.titleCn}
                </h2>
                {locked("titleCn") && (
                  <Lock
                    className="size-3 shrink-0 text-primary/70"
                    aria-label="标题已锁定"
                  />
                )}
              </div>
              <p className="mt-1 truncate text-[11.5px] text-muted-foreground">
                {game.originalTitle}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-1 rounded-md bg-muted px-1.5 py-0.5">
              <Star className="size-3 fill-amber-400 text-amber-400" />
              <span className="text-[12px] font-semibold tabular-nums">
                {game.rating.toFixed(1)}
              </span>
            </div>
          </div>

          <div className="mt-2 flex flex-wrap items-center gap-1">
            {visibleTags.map((t) => (
              <Badge
                key={t}
                variant="secondary"
                className="h-5 rounded-full px-2 text-[10px] font-normal"
              >
                {t}
              </Badge>
            ))}
            {hiddenTagCount > 0 && (
              <button
                type="button"
                onClick={() => setTagsExpanded(true)}
                className="h-5 rounded-full border border-border px-1.5 text-[10px] tabular-nums text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                title={`还有 ${hiddenTagCount} 个标签`}
              >
                +{hiddenTagCount}
              </button>
            )}
            {tagsExpanded && game.tags.length > TAGS_PREVIEW && (
              <button
                type="button"
                onClick={() => setTagsExpanded(false)}
                className="h-5 px-1 text-[10px] text-muted-foreground underline-offset-2 transition-colors hover:text-foreground hover:underline"
              >
                收起
              </button>
            )}
          </div>

          <Separator className="my-3" />

          {!editing ? (
            <div className="space-y-0.5">
              <FieldRow
                label="开发商"
                value={game.developer}
                locked={locked("developer")}
              />
              <FieldRow label="发售日" value={game.released} locked={locked("released")} />
              <FieldRow label="引擎" value={engineLabel(game.engine)} />
              <FieldRow label="平台" value={PLATFORM_LABEL[game.platform]} />
              <FieldRow
                label="评分来源"
                value={game.ratingSource.toUpperCase()}
                locked={locked("rating")}
              />
              <FieldRow label="主程序" value={game.launchExe} mono />
              {/* 文件数与体积合并成一行：本来是两行，占了 30px，
                  把「展开全部」挤到滚动区外面去了。 */}
              <FieldRow
                label="文件"
                value={`${formatCount(game.fileCount)} 个${
                  game.dirSize > 0 ? ` · ${formatBytes(game.dirSize)}` : ""
                }`}
              />
              {/* 体积只有扫描过的条目才有（迁移进来的老数据 dir_size = 0），
                  为 0 时上面那行只显示个数。 */}
              <FieldRow label="入库" value={game.firstSeenAt.slice(0, 10)} />
            </div>
          ) : (
            draft && (
              <div className="space-y-3">
                <EditField label="中文标题">
                  <Input
                    value={draft.titleCn}
                    onChange={(e) => set("titleCn", e.target.value)}
                    className="h-8 text-[13px]"
                  />
                </EditField>
                <EditField label="开发商">
                  <Input
                    value={draft.developer}
                    onChange={(e) => set("developer", e.target.value)}
                    className="h-8 text-[13px]"
                  />
                </EditField>
                <div className="grid grid-cols-2 gap-2">
                  <EditField label="发售日">
                    <Input
                      value={draft.released}
                      onChange={(e) => set("released", e.target.value)}
                      placeholder="YYYY-MM-DD"
                      className="h-8 text-[12px]"
                    />
                  </EditField>
                  <EditField label="评分">
                    <Input
                      value={draft.rating}
                      onChange={(e) => set("rating", e.target.value)}
                      inputMode="decimal"
                      className="h-8 text-[12px]"
                    />
                  </EditField>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  <EditField label="引擎">
                    <Select
                      value={draft.engine}
                      onValueChange={(v) => set("engine", v as Engine)}
                    >
                      <SelectTrigger className="h-8 w-full text-[12px]">
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
                  </EditField>
                  <EditField label="平台">
                    <Select
                      value={draft.platform}
                      onValueChange={(v) => set("platform", v as Platform)}
                    >
                      <SelectTrigger className="h-8 w-full text-[12px]">
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
                  </EditField>
                </div>
                <EditField label="标签（用、或逗号分隔）">
                  <Input
                    value={draft.tags}
                    onChange={(e) => set("tags", e.target.value)}
                    className="h-8 text-[12px]"
                  />
                </EditField>
                <EditField label="简介">
                  <Textarea
                    value={draft.description}
                    onChange={(e) => set("description", e.target.value)}
                    rows={3}
                    className="resize-none text-[12px] leading-relaxed"
                  />
                </EditField>
              </div>
            )
          )}

          {!editing && description && (
            <div className="mt-2 border-t border-border pt-2">
              {/* 旧库的中文简介存在 cn_description 里，英文在 description；
                  中文优先，没有中文才退回英文 */}
              <p
                className={cn(
                  "selectable text-[12px] leading-[1.75] text-muted-foreground",
                  // 长简介先折 4 行：再长就会把「展开全部」顶到滚动区外面，
                  // 用户根本看不见那个按钮。
                  !descExpanded && descIsLong && "line-clamp-[4]",
                )}
              >
                {description}
              </p>
              {descIsLong && (
                <button
                  type="button"
                  onClick={() => setDescExpanded((v) => !v)}
                  className="mt-1 text-[11px] text-primary transition-opacity hover:opacity-75"
                >
                  {descExpanded ? "收起" : "展开全部"}
                </button>
              )}
            </div>
          )}
        </div>
      </ScrollArea>

      <footer className="shrink-0 space-y-2 border-t border-border p-3">
        {editing ? (
          <div className="flex gap-2">
            <Button
              onClick={submit}
              disabled={saving}
              className="h-9 flex-1 gap-1.5"
            >
              {saving ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Save className="size-4" />
              )}
              {saving ? "保存中…" : "保存并锁定"}
            </Button>
            <Button
              variant="outline"
              onClick={onCancelEdit}
              disabled={saving}
              className="h-9 gap-1.5"
            >
              <X className="size-4" />
              取消
            </Button>
          </div>
        ) : (
          <>
            {/* 启动：首次点会先让你确认启动程序（面板弹在按钮上方） */}
            {multi ? (
              <Button disabled className="h-9 w-full gap-1.5">
                <Play className="size-4" />
                启动游戏
              </Button>
            ) : (
              <LaunchControl
                game={game}
                placement="top"
                onGameUpdated={onGameUpdated}
              />
            )}
            <Button
              variant="outline"
              onClick={onPatch}
              disabled={multi}
              className="h-9 w-full gap-1.5"
            >
              <Puzzle className="size-4" />
              补丁管理
            </Button>
            <Button
              variant="secondary"
              onClick={onStartEdit}
              disabled={multi}
              className="h-9 w-full gap-1.5"
            >
              <Pencil className="size-4" />
              编辑信息
            </Button>
          </>
        )}

        {/* 忽略 / 删除：低对比度的文字按钮，不放主按钮行 —— 这不是常用动作，
            但扫描把工具目录收进来时必须一眼能找到。多选时作用于选中的全部。 */}
        {!editing && (
          <div className="flex items-center justify-between border-t border-border pt-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={onIgnore}
              className="h-7 gap-1 px-2 text-[11.5px] text-muted-foreground hover:text-foreground"
            >
              <EyeOff className="size-3.5" />
              忽略{multi ? `这 ${multiCount} 部` : ""}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={onDelete}
              className="h-7 gap-1 px-2 text-[11.5px] text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
            >
              <Trash2 className="size-3.5" />
              删除{multi ? `这 ${multiCount} 部` : ""}
            </Button>
          </div>
        )}

        {/* 路径本身就是"打开文件夹"的入口：比再挤一个按钮省地方 */}
        <button
          type="button"
          onClick={onOpenFolder}
          title={`在资源管理器中打开：${game.folderPath}`}
          className="group flex w-full items-center justify-center gap-1 pt-0.5 text-[10.5px] text-muted-foreground transition-colors hover:text-foreground"
        >
          <FolderOpen className="size-3 shrink-0 opacity-70 group-hover:opacity-100" />
          <span className="truncate">{game.folderPath}</span>
        </button>
      </footer>
      </motion.div>
    </aside>
  );
}
