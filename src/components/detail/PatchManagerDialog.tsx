import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  FolderOpen,
  Loader2,
  Package,
  Play,
  Puzzle,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import {
  openFolder,
  patchApply,
  patchDelete,
  patchInstallerBegin,
  patchInstallerFinish,
  patchList,
  patchPreview,
  patchRevert,
  patchScan,
} from "@/lib/api";
import type { PatchCandidate, PatchPreview, PatchRecord } from "@/lib/api";
import type { Game } from "@/types/game";
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
import { PathPickerDialog } from "@/components/common/PathPickerDialog";
import { formatBytes, formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";

interface PatchManagerDialogProps {
  open: boolean;
  game: Game;
  onOpenChange: (open: boolean) => void;
  /** 装完 / 还原完通知上层刷新游戏行（launchExe 可能变了） */
  onChanged?: () => void;
}

type View = "list" | "preview" | "installer";

const STATUS_LABEL: Record<string, { text: string; cls: string }> = {
  applied: { text: "已应用", cls: "bg-primary/10 text-primary" },
  reverted: { text: "已还原", cls: "bg-muted text-muted-foreground" },
  partial: { text: "部分还原", cls: "bg-destructive/10 text-destructive" },
};

function shortTime(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 16).replace("T", " ");
  return d.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * 补丁管理。
 *
 * 这个对话框要回答用户的三个问题：
 * 1. 这部游戏装过什么补丁？（列表，含已还原的）
 * 2. 我这个补丁会动哪些文件、会不会覆盖主程序？（预览，动手前先算清楚）
 * 3. 装坏了怎么办？（每条记录都能还原，还原不了的文件如实标出来）
 *
 * 设计底线：**预览阶段绝不碰硬盘**。用户点了"应用"才备份 + 覆盖，
 * 而且要能在一个对话框里完成"看见影响 → 确认 → 回退"。
 *
 * 7z / rar 不直接支持：后端会返回一句"先解压再选文件夹"，
 * 这里原样展示，不给"重试"按钮 —— 重试也不会成功。
 */
export function PatchManagerDialog({
  open,
  game,
  onOpenChange,
  onChanged,
}: PatchManagerDialogProps) {
  const [view, setView] = useState<View>("list");
  const [records, setRecords] = useState<PatchRecord[]>([]);
  const [candidates, setCandidates] = useState<PatchCandidate[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  /** 点过一次「还原」的 id：按钮变成"确认还原"，防误触 */
  const [confirmRevert, setConfirmRevert] = useState<string | null>(null);

  const [pickerOpen, setPickerOpen] = useState(false);
  const [preview, setPreview] = useState<PatchPreview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  /** 读补丁失败的原因。以前只弹一个 Toast，弹窗里什么都不显示，看着像"点了没反应" */
  const [previewError, setPreviewError] = useState<string | null>(null);
  /** 应用时的显示名，默认取补丁包名 */
  const [patchName, setPatchName] = useState("");
  /** 勾选"把补丁里的 exe 设为主程序" */
  const [launchExe, setLaunchExe] = useState<string | null>(null);

  /** 安装器会话 */
  const [installerId, setInstallerId] = useState<string | null>(null);
  const [installerName, setInstallerName] = useState("");
  const [installerBusy, setInstallerBusy] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const [list, found] = await Promise.all([patchList(game.id), patchScan(game.id)]);
      setRecords(list);
      setCandidates(found.filter((c) => !c.registered));
    } catch (err) {
      console.error("[patch_list]", err);
      toast.error("读取补丁记录失败", { description: String(err) });
    } finally {
      setLoading(false);
    }
  }, [game.id]);

  useEffect(() => {
    if (!open) return;
    setView("list");
    setPreview(null);
    setInstallerId(null);
    setConfirmRevert(null);
    void reload();
  }, [open, reload]);

  /* ---------------------------------------------------------------- 导入 */

  const choosePatch = async (path: string) => {
    setPreviewing(true);
    setPreviewError(null);
    // 先关掉选择器、切到"正在读取"：大压缩包解压几十秒也不会让人以为点空了
    setPickerOpen(false);
    try {
      const next = await patchPreview(game.id, path);
      setPatchName(next.name);
      setPreview(next);

      if (next.installer) {
        // 安装器：先拍快照再启动，装完由用户点"完成"
        const session = await patchInstallerBegin(game.id, path);
        setInstallerId(session);
        setInstallerName(next.name);
        setView("installer");
      } else {
        setLaunchExe(next.exes.length > 0 ? next.exes[0] : null);
        setView("preview");
      }
    } catch (err) {
      console.error("[patch_preview]", err);
      setPreviewError(String(err));
      setView("list");
      toast.error("读不了这个补丁", { description: String(err) });
    } finally {
      setPreviewing(false);
    }
  };

  const apply = async () => {
    if (!preview) return;
    setBusyId("apply");
    try {
      const rec = await patchApply(game.id, preview.stagingId, patchName, launchExe ?? undefined);
      toast.success("补丁已应用", {
        description: `新增 ${rec.addedCount} 个文件，覆盖 ${rec.replacedCount} 个（已备份）`,
      });
      setView("list");
      setPreview(null);
      await reload();
      onChanged?.();
    } catch (err) {
      console.error("[patch_apply]", err);
      toast.error("应用失败", { description: String(err) });
    } finally {
      setBusyId(null);
    }
  };

  const finishInstaller = async () => {
    if (!installerId) return;
    setInstallerBusy(true);
    try {
      const report = await patchInstallerFinish(game.id, installerId, installerName);
      if (report.changed === 0) {
        toast("没有检测到文件变化", {
          description: "安装器可能装到了别的目录，或中途取消了。",
        });
      } else {
        toast.success(`已记录 ${report.changed} 个文件的变化`, {
          description: "如果装错了，可以在列表里还原。",
        });
      }
      setInstallerId(null);
      setView("list");
      await reload();
      onChanged?.();
    } catch (err) {
      console.error("[patch_installer_finish]", err);
      toast.error("记录失败", { description: String(err) });
    } finally {
      setInstallerBusy(false);
    }
  };

  /* ---------------------------------------------------------------- 还原 */

  const revert = async (rec: PatchRecord) => {
    if (confirmRevert !== rec.id) {
      setConfirmRevert(rec.id);
      window.setTimeout(() => setConfirmRevert((cur) => (cur === rec.id ? null : cur)), 4000);
      return;
    }

    setConfirmRevert(null);
    setBusyId(rec.id);
    try {
      const report = await patchRevert(rec.id);
      const parts = [`恢复 ${report.restored} 个`, `删除 ${report.removed} 个`];
      if (report.skipped > 0) parts.push(`${report.skipped} 个无法还原（当时没备份）`);

      if (report.skipped > 0) {
        toast.warning("已尽量还原", { description: parts.join(" · ") });
      } else {
        toast.success("补丁已还原", { description: parts.join(" · ") });
      }
      await reload();
      onChanged?.();
    } catch (err) {
      console.error("[patch_revert]", err);
      toast.error("还原失败", { description: String(err) });
    } finally {
      setBusyId(null);
    }
  };

  const remove = async (rec: PatchRecord) => {
    setBusyId(rec.id);
    try {
      await patchDelete(rec.id);
      toast.success("记录已删除", { description: "游戏目录里的文件没动。" });
      await reload();
    } catch (err) {
      console.error("[patch_delete]", err);
      toast.error("删除失败", { description: String(err) });
    } finally {
      setBusyId(null);
    }
  };

  /* ---------------------------------------------------------------- 渲染 */

  const appliedCount = useMemo(
    () => records.filter((r) => r.status === "applied").length,
    [records],
  );

  const renderList = () => (
    <div className="space-y-4">
      {/* 读取中 / 读取失败的显性反馈。
          之前这两件事只有 Toast，弹窗里一片安静 —— 用户的原话就是"选了没有任何反应"。 */}
      {previewing && (
        <div className="flex items-center gap-2 rounded-lg bg-accent px-3 py-2 text-[11.5px] text-accent-foreground">
          <Loader2 className="size-3.5 shrink-0 animate-spin" />
          正在读取补丁…（压缩包要先解压，大包可能要几十秒）
        </div>
      )}

      {previewError && !previewing && (
        <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2.5 text-[11.5px] text-destructive">
          <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
          <div className="min-w-0 flex-1">
            <p className="font-medium">这个补丁读不了</p>
            <p className="mt-0.5 leading-relaxed break-all text-destructive/80">
              {previewError}
            </p>
            <Button
              variant="outline"
              size="sm"
              className="mt-2 h-7 px-2 text-[11.5px]"
              onClick={() => {
                setPreviewError(null);
                setPickerOpen(true);
              }}
            >
              换一个补丁
            </Button>
          </div>
        </div>
      )}

      <div className="flex items-center justify-between gap-3">
        <p className="text-[11.5px] text-muted-foreground">
          {loading
            ? "读取中…"
            : records.length === 0
              ? "还没有补丁记录"
              : `${records.length} 条记录 · ${appliedCount} 条生效中`}
        </p>
        <Button
          size="sm"
          className="h-8 shrink-0 gap-1.5"
          disabled={previewing}
          onClick={() => setPickerOpen(true)}
        >
          {previewing ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Package className="size-3.5" />
          )}
          {previewing ? "正在读取…" : "导入补丁"}
        </Button>
      </div>

      {loading ? (
        <div className="flex h-28 items-center justify-center rounded-xl border border-border bg-card">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      ) : records.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border px-5 py-8 text-center">
          <p className="text-[12.5px] font-medium">这部游戏还没有装过补丁</p>
          <p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground">
            点右上角「导入补丁」，可以选 zip、已经解压好的文件夹，或安装器 exe。
            <br />
            应用前会先列出会覆盖哪些文件，并自动备份原文件。
          </p>
        </div>
      ) : (
        <ul className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
          {records.map((rec) => {
            const badge = STATUS_LABEL[rec.status] ?? STATUS_LABEL.applied;
            const busy = busyId === rec.id;
            return (
              <li key={rec.id} className="px-4 py-3">
                <div className="flex items-start gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-[12.5px] font-medium text-foreground">
                        {rec.name}
                      </span>
                      <span
                        className={cn(
                          "rounded px-1.5 py-[1px] text-[10px] font-medium",
                          badge.cls,
                        )}
                      >
                        {badge.text}
                      </span>
                      {rec.kind === "installer" && (
                        <span className="rounded bg-muted px-1.5 py-[1px] text-[10px] text-muted-foreground">
                          安装器
                        </span>
                      )}
                    </div>
                    <p className="mt-1 text-[11px] text-muted-foreground">
                      {rec.fileCount} 个文件 · 新增 {rec.addedCount} · 覆盖 {rec.replacedCount}
                      {rec.unbackedCount > 0 && (
                        <span className="text-destructive"> · {rec.unbackedCount} 个未备份</span>
                      )}
                      {rec.appliedAt && ` · ${shortTime(rec.appliedAt)}`}
                    </p>
                    <p className="mt-0.5 truncate text-[10.5px] text-muted-foreground/70" title={rec.sourcePath}>
                      {rec.sourcePath}
                    </p>
                  </div>

                  <div className="flex shrink-0 items-center gap-1.5">
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label="打开补丁来源位置"
                      className="size-7 text-muted-foreground"
                      onClick={() => {
                        const dir = rec.sourcePath.replace(/[\\/][^\\/]+$/, "");
                        void openFolder(dir || game.folderPath).catch(() => {
                          toast.error("打不开这个位置");
                        });
                      }}
                    >
                      <FolderOpen className="size-3.5" />
                    </Button>
                    {rec.status === "applied" && (
                      <Button
                        variant={confirmRevert === rec.id ? "destructive" : "outline"}
                        size="sm"
                        className="h-7 shrink-0 gap-1 px-2 text-[11.5px]"
                        disabled={busy}
                        onClick={() => void revert(rec)}
                      >
                        {busy ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <RotateCcw className="size-3.5" />
                        )}
                        {confirmRevert === rec.id ? "确认还原" : "还原"}
                      </Button>
                    )}
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label="删除记录"
                      className="size-7 text-muted-foreground hover:text-destructive"
                      disabled={busy}
                      onClick={() => void remove(rec)}
                    >
                      <Trash2 className="size-3.5" />
                    </Button>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {candidates.length > 0 && (
        <div>
          <p className="mb-2 text-[11.5px] font-medium text-foreground">
            游戏目录里发现这些补丁（还没登记）
          </p>
          <ul className="divide-y divide-border overflow-hidden rounded-xl border border-dashed border-border">
            {candidates.map((c) => (
              <li key={c.path} className="flex items-center gap-3 px-4 py-2.5">
                <Puzzle className="size-3.5 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12px] text-foreground">{c.relPath}</p>
                  <p className="text-[10.5px] text-muted-foreground">
                    {c.files} 个文件 · {formatBytes(c.size)}
                  </p>
                </div>
                <Button
                  size="sm"
                  className="h-7 shrink-0 gap-1 px-2 text-[11px]"
                  disabled={previewing}
                  onClick={() => void choosePatch(c.path)}
                >
                  <Package className="size-3" />
                  预览
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 shrink-0 px-2 text-[11px]"
                  onClick={() => {
                    void openFolder(c.path).catch(() => toast.error("打不开这个目录"));
                  }}
                >
                  打开位置
                </Button>
              </li>
            ))}
          </ul>
          <p className="mt-1.5 text-[10.5px] leading-relaxed text-muted-foreground">
            这些目录是扫描出来的，不代表装没装过。要重新应用就点「导入补丁」选中它，
            软件会先备份再覆盖。
          </p>
        </div>
      )}
    </div>
  );

  const renderPreview = () => {
    if (!preview) return null;

    return (
      <div className="space-y-4">
        <div className="rounded-xl border border-border bg-card px-4 py-3.5">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="text-[12.5px] font-medium">这个补丁会改动游戏目录</p>
              <p className="mt-0.5 truncate text-[11px] text-muted-foreground" title={preview.source}>
                {preview.source}
              </p>
            </div>
            <span className="shrink-0 text-[11px] text-muted-foreground">
              {formatBytes(preview.totalBytes)}
            </span>
          </div>

          <div className="mt-3 flex gap-2">
            <span className="rounded-md bg-primary/10 px-2 py-1 text-[11px] text-primary">
              新增 {preview.added}
            </span>
            <span className="rounded-md bg-muted px-2 py-1 text-[11px] text-foreground">
              覆盖 {preview.replaced}
            </span>
            {preview.unbacked > 0 && (
              <span className="rounded-md bg-destructive/10 px-2 py-1 text-[11px] text-destructive">
                {preview.unbacked} 个超过 200MB，不备份
              </span>
            )}
          </div>
        </div>

        <div>
          <Label className="text-[11px] text-muted-foreground">补丁名称</Label>
          <Input
            value={patchName}
            onChange={(e) => setPatchName(e.target.value)}
            className="mt-1 h-8 text-[12.5px]"
          />
        </div>

        {preview.exes.length > 0 && (
          <div className="rounded-lg border border-border bg-muted/30 px-3 py-2.5">
            <Label className="text-[11px] text-muted-foreground">
              补丁里带了可执行文件，要不要同时改启动项？
            </Label>
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              {preview.exes.map((exe) => (
                <button
                  key={exe}
                  type="button"
                  onClick={() => setLaunchExe((cur) => (cur === exe ? null : exe))}
                  className={cn(
                    "rounded-md border px-2 py-1 text-[11px] transition-colors",
                    launchExe === exe
                      ? "border-primary bg-primary/10 text-primary"
                      : "border-border text-muted-foreground hover:text-foreground",
                  )}
                >
                  {launchExe === exe && <Check className="mr-0.5 inline size-3" />}
                  {exe}
                </button>
              ))}
            </div>
            <p className="mt-1.5 text-[10.5px] text-muted-foreground">
              选中后会把「启动游戏」指向它（补丁里的汉化版主程序通常就是这个）。
            </p>
          </div>
        )}

        <div>
          <p className="mb-1.5 text-[11px] text-muted-foreground">
            文件清单{preview.truncated ? `（只显示前 ${preview.files.length} 个）` : ""}
          </p>
          <ul className="max-h-[190px] divide-y divide-border overflow-y-auto rounded-lg border border-border">
            {preview.files.map((f) => (
              <li key={f.relPath} className="flex items-center gap-2 px-3 py-[5px]">
                <span
                  className={cn(
                    "w-[34px] shrink-0 rounded px-1 text-center text-[9.5px]",
                    f.action === "replace"
                      ? "bg-muted text-foreground"
                      : "bg-primary/10 text-primary",
                  )}
                >
                  {f.action === "replace" ? "覆盖" : "新增"}
                </span>
                <span className="min-w-0 flex-1 truncate text-[11px] text-foreground" title={f.relPath}>
                  {f.relPath}
                </span>
                {!f.backup && (
                  <span className="shrink-0 text-[10px] text-destructive">不备份</span>
                )}
                <span className="shrink-0 text-[10.5px] tabular-nums text-muted-foreground">
                  {formatBytes(f.size)}
                </span>
              </li>
            ))}
          </ul>
        </div>

        {preview.unbacked > 0 && (
          <p className="flex gap-1.5 rounded-lg bg-destructive/10 px-3 py-2 text-[11px] leading-relaxed text-destructive">
            <AlertTriangle className="mt-[1px] size-3.5 shrink-0" />
            有 {preview.unbacked} 个文件超过 200MB 不会被备份，还原时它们回不到原样。
            如果这个补丁只是换语言包，通常不影响；换了整包资源就要想清楚。
          </p>
        )}
      </div>
    );
  };

  const renderInstaller = () => (
    <div className="space-y-4">
      <div className="rounded-xl border border-border bg-card px-4 py-4">
        <div className="flex items-center gap-2">
          <Play className="size-4 text-primary" />
          <p className="text-[12.5px] font-medium">安装器已经启动</p>
        </div>
        <p className="mt-1.5 text-[11.5px] leading-relaxed text-muted-foreground">
          请在安装向导里完成安装（一般直接点下一步，目录就是游戏目录）。
          <br />
          装完后回到这里点「安装完成」—— 软件会对比安装前后的目录，
          把变化的文件记下来，以后可以还原。
        </p>
        <p className="mt-2 truncate text-[10.5px] text-muted-foreground/70" title={preview?.source}>
          {preview?.source}
        </p>
      </div>

      <div>
        <Label className="text-[11px] text-muted-foreground">补丁名称</Label>
        <Input
          value={installerName}
          onChange={(e) => setInstallerName(e.target.value)}
          className="mt-1 h-8 text-[12.5px]"
        />
      </div>
    </div>
  );

  const footer = () => {
    if (view === "preview") {
      return (
        <>
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            onClick={() => {
              setPreview(null);
              setView("list");
            }}
          >
            取消
          </Button>
          <Button
            size="sm"
            className="h-8 gap-1.5"
            disabled={busyId === "apply"}
            onClick={() => void apply()}
          >
            {busyId === "apply" ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Package className="size-3.5" />
            )}
            {busyId === "apply" ? "正在应用…" : "应用补丁"}
          </Button>
        </>
      );
    }

    if (view === "installer") {
      return (
        <>
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            onClick={() => {
              setInstallerId(null);
              setPreview(null);
              setView("list");
            }}
          >
            取消
          </Button>
          <Button
            size="sm"
            className="h-8 gap-1.5"
            disabled={installerBusy || !installerId}
            onClick={() => void finishInstaller()}
          >
            {installerBusy ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Check className="size-3.5" />
            )}
            {installerBusy ? "正在对比…" : "安装完成"}
          </Button>
        </>
      );
    }

    return (
      <Button variant="outline" size="sm" className="h-8" onClick={() => onOpenChange(false)}>
        关闭
      </Button>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-[720px]">
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="text-[15px]">
            {view === "list" ? "补丁管理" : view === "preview" ? "确认这个补丁" : "安装器补丁"}
          </DialogTitle>
          <DialogDescription className="text-[11.5px] leading-relaxed">
            {view === "list"
              ? "记录这部游戏装过的补丁；每条都能还原到安装前的状态。"
              : view === "preview"
                ? "下面是应用后的实际结果，还没动任何文件。"
                : "安装器自己会写文件，软件通过前后对比来记录改动。"}
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[480px] min-h-[220px] overflow-y-auto px-5 py-4">
          {view === "list"
            ? renderList()
            : view === "preview"
              ? renderPreview()
              : renderInstaller()}
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-border px-5 py-3">
          <p className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
            {view === "list"
              ? `${formatCount(records.length)} 条记录`
              : view === "preview"
                ? `共 ${preview?.totalFiles ?? 0} 个文件`
                : "装完记得回来点「安装完成」"}
          </p>
          <div className="flex gap-2">{footer()}</div>
        </div>
      </DialogContent>

      <PathPickerDialog
        open={pickerOpen}
        mode="patch"
        title="选择补丁"
        onOpenChange={setPickerOpen}
        onPick={(path) => void choosePatch(path)}
      />
    </Dialog>
  );
}
