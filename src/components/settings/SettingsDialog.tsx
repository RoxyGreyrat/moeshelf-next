import { useEffect, useState } from "react";
import {
  FolderOpen,
  FolderPlus,
  HardDrive,
  ImageIcon,
  Loader2,
  RotateCcw,
  ScanLine,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import {
  addScanRoot,
  getAppInfo,
  listIgnoredGames,
  listScanRoots,
  openFolder,
  pickFolder,
  removeScanRoot,
  setGamesIgnored,
} from "@/lib/api";
import type { AppInfo, ScanRoot } from "@/lib/api";
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
import { Separator } from "@/components/ui/separator";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 恢复了某一条之后通知上层重拉列表 */
  onRestored: () => void;
  /** 增删扫描根目录之后通知上层刷新顶栏下拉 */
  onRootsChanged: () => void;
  /** 从设置里直接扫某个已登记目录 */
  onScanRoot: (path: string) => void;
}

/** `E:\galgame\yuzusoft` → `yuzusoft`；`E:\` → `E:\` */
function baseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= 1) return path;
  return parts[parts.length - 1];
}

function formatBytes(n: number): string {
  if (!n) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = n;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i += 1;
  }
  return `${value.toFixed(i === 0 || value >= 100 ? 0 : 1)} ${units[i]}`;
}

function shortDate(iso: string | null): string {
  if (!iso) return "从未扫描";
  return iso.slice(0, 10);
}

function SectionTitle({
  title,
  hint,
  count,
}: {
  title: string;
  hint?: string;
  count?: string;
}) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <div className="min-w-0">
        <h3 className="text-[12px] font-medium text-foreground">{title}</h3>
        {hint && (
          <p className="mt-0.5 text-[11px] leading-relaxed text-muted-foreground">
            {hint}
          </p>
        )}
      </div>
      {count && (
        <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">
          {count}
        </span>
      )}
    </div>
  );
}

/**
 * 设置面板，三块：
 *   1. 扫描根目录 —— 登记过的游戏库目录，顶栏扫描按钮会列出来
 *   2. 已忽略 —— 忽略是隐藏而不是删除，必须能看见、能恢复
 *   3. 数据与关于 —— 库文件在哪、多大、封面缓存有多少张
 *
 * 刮削源、代理、凭据这些等 D 阶段有了后端再往这里加；
 * 现在**不放任何还没有后端的开关**，避免出现点了没反应的假控件。
 */
export function SettingsDialog({
  open,
  onOpenChange,
  onRestored,
  onRootsChanged,
  onScanRoot,
}: SettingsDialogProps) {
  const [ignored, setIgnored] = useState<Game[]>([]);
  const [roots, setRoots] = useState<ScanRoot[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

  // 每次打开都重读：忽略/扫描根可能是在别处（扫描后清理、顶栏扫描）刚变化的
  useEffect(() => {
    if (!open) return;
    let alive = true;
    setLoading(true);

    void (async () => {
      const [ignoredRes, rootsRes, infoRes] = await Promise.allSettled([
        listIgnoredGames(),
        listScanRoots(),
        getAppInfo(),
      ]);
      if (!alive) return;

      if (ignoredRes.status === "fulfilled") setIgnored(ignoredRes.value);
      else {
        console.error("[list_ignored_games]", ignoredRes.reason);
        toast.error("读取忽略清单失败", { description: String(ignoredRes.reason) });
      }

      if (rootsRes.status === "fulfilled") setRoots(rootsRes.value);
      else console.error("[list_scan_roots]", rootsRes.reason);

      if (infoRes.status === "fulfilled") setInfo(infoRes.value);
      else console.error("[app_info]", infoRes.reason);

      setLoading(false);
    })();

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

  const addRoot = async () => {
    let dir: string | null = null;
    try {
      dir = await pickFolder("选择游戏库根目录");
    } catch (err) {
      toast.error("打不开文件夹选择框", { description: String(err) });
      return;
    }
    if (!dir) return;

    setAdding(true);
    try {
      const root = await addScanRoot(dir);
      setRoots((prev) =>
        prev.some((r) => r.id === root.id) ? prev : [...prev, root],
      );
      onRootsChanged();
      toast.success("已登记目录", { description: root.path });
    } catch (err) {
      console.error("[add_scan_root]", err);
      toast.error("登记失败", { description: String(err) });
    } finally {
      setAdding(false);
    }
  };

  const removeRoot = async (r: ScanRoot) => {
    setBusyId(`root-${r.id}`);
    try {
      await removeScanRoot(r.id);
      setRoots((prev) => prev.filter((x) => x.id !== r.id));
      onRootsChanged();
      toast.success("已移除目录", {
        description: "只删登记记录，书架里的游戏和硬盘文件都不动。",
      });
    } catch (err) {
      console.error("[remove_scan_root]", err);
      toast.error("移除失败", { description: String(err) });
    } finally {
      setBusyId(null);
    }
  };

  const openPath = async (path: string, label: string) => {
    try {
      await openFolder(path);
    } catch (err) {
      console.error("[open_folder]", err);
      toast.error(`打不开${label}`, { description: String(err) });
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[600px]">
        <DialogHeader>
          <DialogTitle>设置</DialogTitle>
          <DialogDescription>
            扫描目录、被忽略的条目，以及数据都存在哪。
          </DialogDescription>
        </DialogHeader>

        <ScrollArea className="-mx-1 max-h-[70vh] px-1">
          <div className="space-y-4 pb-2">
            {/* ------------------------------------------------ 扫描根目录 */}
            <section className="space-y-2">
              <SectionTitle
                title="扫描根目录"
                hint="顶栏「扫描」会列出这些目录；扫过一次的目录会自动登记。"
                count={loading ? "…" : `${roots.length} 个`}
              />

              {loading ? (
                <div className="flex h-16 items-center justify-center">
                  <Loader2 className="size-4 animate-spin text-muted-foreground" />
                </div>
              ) : roots.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border px-3 py-5 text-center text-[12px] text-muted-foreground">
                  还没有登记目录。
                  <br />
                  点下面「添加目录」，或直接用顶栏的「扫描文件夹」扫一次。
                </p>
              ) : (
                <ul className="divide-y divide-border rounded-lg border border-border">
                  {roots.map((r) => (
                    <li key={r.id} className="flex items-center gap-2 px-3 py-2">
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-[12.5px] font-medium text-foreground">
                          {baseName(r.path)}
                        </p>
                        <p
                          className="truncate text-[11px] text-muted-foreground"
                          title={r.path}
                        >
                          {r.path}
                        </p>
                      </div>
                      <span className="shrink-0 text-[10.5px] tabular-nums text-muted-foreground">
                        {shortDate(r.lastScanAt)}
                      </span>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 shrink-0 gap-1 px-2 text-[11.5px]"
                        disabled={busyId === `root-${r.id}`}
                        onClick={() => {
                          onOpenChange(false);
                          onScanRoot(r.path);
                        }}
                      >
                        <ScanLine className="size-3.5" />
                        扫描
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        aria-label={`移除 ${r.path}`}
                        className="size-7 shrink-0 text-muted-foreground hover:text-destructive"
                        disabled={busyId === `root-${r.id}`}
                        onClick={() => removeRoot(r)}
                      >
                        {busyId === `root-${r.id}` ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <Trash2 className="size-3.5" />
                        )}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}

              <Button
                variant="outline"
                size="sm"
                onClick={addRoot}
                disabled={adding}
                className="h-8 gap-1.5"
              >
                {adding ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <FolderPlus className="size-3.5" />
                )}
                添加目录
              </Button>
            </section>

            <Separator />

            {/* ---------------------------------------------------- 已忽略 */}
            <section className="space-y-2">
              <SectionTitle
                title="已忽略"
                hint="忽略只是从书架隐藏；重扫不会复活，恢复后立刻回来。"
                count={loading ? "…" : `${ignored.length} 条`}
              />

              {loading ? (
                <div className="flex h-16 items-center justify-center">
                  <Loader2 className="size-4 animate-spin text-muted-foreground" />
                </div>
              ) : ignored.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border px-3 py-5 text-center text-[12px] text-muted-foreground">
                  没有被忽略的条目。
                  <br />
                  在详情面板下方点「忽略」，把工具目录这类误识别的东西藏起来。
                </p>
              ) : (
                <ScrollArea className="max-h-[220px] rounded-lg border border-border">
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
            </section>

            <Separator />

            {/* ------------------------------------------------ 数据与关于 */}
            <section className="space-y-2">
              <SectionTitle
                title="数据与关于"
                hint="MoeShelf Next 是本地应用，所有数据都在你自己机器上。"
              />

              {info ? (
                <div className="space-y-1.5 rounded-lg border border-border bg-muted/30 px-3 py-2.5">
                  <div className="flex items-baseline justify-between gap-3 text-[11.5px]">
                    <span className="text-muted-foreground">版本</span>
                    <span className="tabular-nums text-foreground">
                      v{info.version} · schema v{info.schemaVersion}
                    </span>
                  </div>

                  <div className="flex items-baseline justify-between gap-3 text-[11.5px]">
                    <span className="shrink-0 text-muted-foreground">数据库</span>
                    <span
                      className="selectable min-w-0 truncate text-right text-foreground"
                      title={info.dbPath}
                    >
                      {formatBytes(info.dbSize)}
                    </span>
                  </div>

                  <div className="flex items-baseline justify-between gap-3 text-[11.5px]">
                    <span className="shrink-0 text-muted-foreground">封面缓存</span>
                    <span className="text-right text-foreground">
                      {info.coverDirExists ? (
                        `${info.coverCount.toLocaleString()} 张`
                      ) : (
                        <span className="text-amber-600">目录不存在</span>
                      )}
                    </span>
                  </div>

                  <div className="grid grid-cols-2 gap-1.5 pt-1">
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-7 gap-1.5 px-2 text-[11.5px]"
                      onClick={() => openPath(info.dataDir, "数据目录")}
                    >
                      <HardDrive className="size-3.5 shrink-0" />
                      打开数据目录
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-7 gap-1.5 px-2 text-[11.5px]"
                      disabled={!info.coverDirExists}
                      onClick={() => openPath(info.coverDir, "封面缓存目录")}
                    >
                      <ImageIcon className="size-3.5 shrink-0" />
                      打开封面缓存
                    </Button>
                  </div>

                  <p className="selectable pt-0.5 text-[10.5px] leading-relaxed text-muted-foreground/80">
                    {info.dbPath}
                  </p>
                </div>
              ) : (
                <div className="flex h-16 items-center justify-center rounded-lg border border-dashed border-border">
                  {loading ? (
                    <Loader2 className="size-4 animate-spin text-muted-foreground" />
                  ) : (
                    <span className="text-[11.5px] text-muted-foreground">
                      读取失败
                    </span>
                  )}
                </div>
              )}

              <p className="pt-0.5 text-[11px] text-muted-foreground/80">
                刮削源、代理与凭据会在后续阶段加到这里。
              </p>
            </section>
          </div>
        </ScrollArea>

        <div className="flex items-center justify-between pt-1">
          <span className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
            <FolderOpen className="size-3.5" />
            库文件在本地，不上传。
          </span>
          <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
            关闭
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
