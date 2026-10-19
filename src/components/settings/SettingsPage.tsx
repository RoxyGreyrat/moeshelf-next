import { useEffect, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  ArrowLeft,
  Database,
  EyeOff,
  FolderTree,
  HardDrive,
  ImageIcon,
  Info,
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
  removeScanRoot,
  setGamesIgnored,
} from "@/lib/api";
import type { AppInfo, ScanRoot } from "@/lib/api";
import type { Game } from "@/types/game";
import { Button } from "@/components/ui/button";
import { formatBytes, formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";
import { PathPickerDialog } from "@/components/common/PathPickerDialog";

/* ------------------------------------------------------------------ 类型 */

type TabKey = "roots" | "ignored" | "storage" | "about";

interface NavItem {
  key: TabKey;
  label: string;
  icon: LucideIcon;
}

interface NavGroup {
  group: string;
  items: NavItem[];
}

/**
 * 分区按「用户要办的事」划分，不按技术模块划分。
 *
 * 刻意只保留四个——每一个都有真实后端。
 * 空的「界面显示」「代理」「手机串流」这类不往导航里塞：
 * 点进去只能看到一句"还没做"，除了显得功能多没有任何用处，
 * 原版设置页的混乱正是这么来的。
 */
const NAV: NavGroup[] = [
  {
    group: "游戏库",
    items: [
      { key: "roots", label: "游戏目录", icon: FolderTree },
      { key: "ignored", label: "已忽略", icon: EyeOff },
    ],
  },
  {
    group: "数据",
    items: [{ key: "storage", label: "存储与缓存", icon: HardDrive }],
  },
  {
    group: "关于",
    items: [{ key: "about", label: "版本信息", icon: Info }],
  },
];

const TAB_META: Record<TabKey, { title: string; desc: string }> = {
  roots: {
    title: "游戏目录",
    desc: "扫描这些目录下的一级子文件夹，每个子文件夹当成一部游戏。扫过一次的目录会自动登记。",
  },
  ignored: {
    title: "已忽略",
    desc: "忽略只是从书架隐藏，记录还在库里。重新扫描同一个目录也不会再冒出来。",
  },
  storage: {
    title: "存储与缓存",
    desc: "数据库和封面缓存都在本机，不上传任何服务器。",
  },
  about: {
    title: "版本信息",
    desc: "当前版本与运行环境。",
  },
};

interface SettingsPageProps {
  /** 页头副标题里的游戏总数 */
  gameCount: number;
  onClose: () => void;
  /** 恢复了某一条之后通知上层重拉列表 */
  onRestored: () => void;
  /** 增删扫描根目录之后通知上层刷新顶栏下拉 */
  onRootsChanged: () => void;
  /** 从设置里直接扫某个已登记目录 */
  onScanRoot: (path: string) => void;
}

/* ------------------------------------------------------------------ 小工具 */

/** `E:\galgame\yuzusoft` → `yuzusoft`；`E:\` → `E:\` */
function baseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= 1) return path;
  return parts[parts.length - 1];
}

function shortDate(iso: string | null): string {
  if (!iso) return "从未扫描";
  return iso.slice(0, 10);
}

/** 分区外壳：标题 + 说明 + 一块白色卡片 */
function Section({
  title,
  hint,
  action,
  children,
}: {
  title: string;
  hint?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-2.5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h3 className="text-[12.5px] font-medium text-foreground">{title}</h3>
          {hint && (
            <p className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">
              {hint}
            </p>
          )}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

/** 两列键值行：左侧灰字，右侧深字 */
function InfoRow({
  label,
  value,
  mono,
  title,
}: {
  label: string;
  value: React.ReactNode;
  mono?: boolean;
  title?: string;
}) {
  return (
    <div className="flex items-baseline justify-between gap-6 border-b border-border py-2 last:border-b-0">
      <span className="shrink-0 text-[11.5px] text-muted-foreground">{label}</span>
      <span
        title={title}
        className={cn(
          "min-w-0 truncate text-right text-[12px] text-foreground",
          mono && "tabular-nums",
        )}
      >
        {value}
      </span>
    </div>
  );
}

/** 空列表提示 */
function EmptyHint({ children }: { children: React.ReactNode }) {
  return (
    <p className="rounded-xl border border-dashed border-border px-4 py-8 text-center text-[12px] leading-relaxed text-muted-foreground">
      {children}
    </p>
  );
}

/* ------------------------------------------------------------------ 主体 */

export function SettingsPage({
  gameCount,
  onClose,
  onRestored,
  onRootsChanged,
  onScanRoot,
}: SettingsPageProps) {
  const [tab, setTab] = useState<TabKey>("roots");
  const [ignored, setIgnored] = useState<Game[]>([]);
  const [roots, setRoots] = useState<ScanRoot[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);

  // 只读一次；设置页是独立的整页，不存在「关掉再开」的缓存问题
  useEffect(() => {
    let alive = true;

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
  }, []);

  // Esc 退出设置
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

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

  /** 用自制的目录选择器挑目录（不弹系统对话框） */
  const addRoot = () => setPickerOpen(true);

  const confirmRoot = async (dir: string) => {
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

  /* ---------------------------------------------------------------- 各分区 */

  const renderRoots = () => (
    <div className="space-y-2.5">
      <div className="flex items-center justify-between gap-4">
        <p className="text-[11.5px] text-muted-foreground">
          {loading ? "读取中…" : `共 ${roots.length} 个目录`}
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={addRoot}
          disabled={adding}
          className="h-8 shrink-0 gap-1.5"
        >
          {adding ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <FolderTree className="size-3.5" />
          )}
          添加目录
        </Button>
      </div>

      {loading ? (
        <div className="flex h-24 items-center justify-center rounded-xl border border-border bg-card">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      ) : roots.length === 0 ? (
        <EmptyHint>
          还没有登记目录。
          <br />
          点右上角「添加目录」，或者直接用顶栏的「扫描文件夹」扫一次。
        </EmptyHint>
      ) : (
        <ul className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
          {roots.map((r) => (
            <li key={r.id} className="flex items-center gap-3 px-4 py-3">
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground">
                <FolderTree className="size-4" />
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-[12.5px] font-medium text-foreground">
                  {baseName(r.path)}
                </p>
                <p
                  className="mt-0.5 truncate text-[11px] text-muted-foreground"
                  title={r.path}
                >
                  {r.path}
                </p>
              </div>
              <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">
                {shortDate(r.lastScanAt)}
              </span>
              <Button
                variant="outline"
                size="sm"
                className="h-7 shrink-0 gap-1 px-2 text-[11.5px]"
                disabled={busyId === `root-${r.id}`}
                onClick={() => {
                  onClose();
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
    </div>
  );

  const renderIgnored = () => (
    <div className="space-y-2.5">
      <p className="text-[11.5px] text-muted-foreground">
        {loading ? "读取中…" : `共 ${ignored.length} 条`}
      </p>

      {loading ? (
        <div className="flex h-24 items-center justify-center rounded-xl border border-border bg-card">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      ) : ignored.length === 0 ? (
        <EmptyHint>
          没有被忽略的条目。
          <br />
          在详情面板下方点「忽略」，可以把工具目录这类误识别的东西藏起来。
        </EmptyHint>
      ) : (
        <ul className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
          {ignored.map((g) => (
            <li key={g.id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <p className="truncate text-[12.5px] font-medium text-foreground">
                  {g.titleCn || g.folderPath}
                </p>
                <p
                  className="mt-0.5 truncate text-[11px] text-muted-foreground"
                  title={g.folderPath}
                >
                  {g.folderPath}
                </p>
              </div>
              <Button
                variant="outline"
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
      )}
    </div>
  );

  const renderStorage = () => (
    <div className="space-y-6">
      <Section title="数据库" hint="游戏条目、游玩时长、人工锁定的字段都存在这个文件里。">
        {info ? (
          <div className="rounded-xl border border-border bg-card px-4 py-1">
            <InfoRow
              label="文件"
              value={info.dbPath}
              title={info.dbPath}
            />
            <InfoRow label="大小" value={formatBytes(info.dbSize)} mono />
            <InfoRow label="结构版本" value={`schema v${info.schemaVersion}`} mono />
            <div className="py-2.5">
              <Button
                variant="outline"
                size="sm"
                className="h-7 gap-1.5 px-2 text-[11.5px]"
                onClick={() => openPath(info.dataDir, "数据目录")}
              >
                <HardDrive className="size-3.5" />
                打开数据目录
              </Button>
            </div>
          </div>
        ) : (
          <div className="flex h-24 items-center justify-center rounded-xl border border-dashed border-border">
            {loading ? (
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            ) : (
              <span className="text-[11.5px] text-muted-foreground">读取失败</span>
            )}
          </div>
        )}
      </Section>

      <Section
        title="封面缓存"
        hint="当前直读旧项目的缓存目录，不复制、不重新下载。"
      >
        {info ? (
          <div className="rounded-xl border border-border bg-card px-4 py-1">
            <InfoRow label="张数" value={info.coverDirExists ? `${formatCount(info.coverCount)} 张` : "目录不存在"} mono />
            <InfoRow label="目录" value={info.coverDir} title={info.coverDir} />
            <div className="py-2.5">
              <Button
                variant="outline"
                size="sm"
                className="h-7 gap-1.5 px-2 text-[11.5px]"
                disabled={!info.coverDirExists}
                onClick={() => openPath(info.coverDir, "封面缓存目录")}
              >
                <ImageIcon className="size-3.5" />
                打开封面缓存
              </Button>
            </div>
          </div>
        ) : null}
      </Section>
    </div>
  );

  const renderAbout = () => (
    <div className="space-y-6">
      <Section title="版本">
        <div className="rounded-xl border border-border bg-card px-4 py-3">
          <div className="flex items-center gap-3">
            <span className="flex size-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
              <Database className="size-5" />
            </span>
            <div>
              <p className="text-[14px] font-semibold tracking-tight">
                MoeShelf Next
              </p>
              <p className="text-[11.5px] text-muted-foreground">
                {info ? `v${info.version} · schema v${info.schemaVersion}` : "读取中…"}
              </p>
            </div>
          </div>
          <p className="mt-3 border-t border-border pt-3 text-[11.5px] leading-relaxed text-muted-foreground">
            本地优先的 Galgame 库管理器。扫描、启动、补丁都在本机完成，
            不依赖任何后端服务，数据不上传。
          </p>
        </div>
      </Section>

      <Section title="运行环境">
        <div className="rounded-xl border border-border bg-card px-4 py-1">
          <InfoRow label="界面" value="Tauri v2 + React 19" />
          <InfoRow label="数据库" value="SQLite（本地文件）" />
          <InfoRow label="平台" value="Windows" />
        </div>
      </Section>
    </div>
  );

  /** 还没接入的分区：这一版不往导航里放，函数留作模板 */
  const renderContent = () => {
    switch (tab) {
      case "roots":
        return renderRoots();
      case "ignored":
        return renderIgnored();
      case "storage":
        return renderStorage();
      default:
        return renderAbout();
    }
  };

  const meta = TAB_META[tab];

  /* ------------------------------------------------------------------ 渲染 */

  return (
    <div className="flex h-full flex-col overflow-hidden bg-background text-foreground">
      {/* 页头：返回 + 标题 + 统计 */}
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border px-5">
        <Button
          variant="ghost"
          size="sm"
          onClick={onClose}
          className="h-8 gap-1.5 px-2 text-[12px] text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="size-4" />
          返回书架
        </Button>
        <span className="h-4 w-px bg-border" />
        <h1 className="text-[14px] font-semibold tracking-tight">设置</h1>
        <p className="truncate text-[11.5px] text-muted-foreground">
          {`${formatCount(gameCount)} 个游戏 · 数据保存在本机`}
        </p>
        <span className="ml-auto hidden text-[11px] text-muted-foreground/70 sm:block">
          Esc 返回
        </span>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* 左侧分栏 */}
        <nav className="w-56 shrink-0 overflow-y-auto border-r border-border px-3 py-4">
          {NAV.map((group) => (
            <div key={group.group} className="mb-4 last:mb-0">
              <p className="mb-1.5 px-2 text-[10.5px] font-medium tracking-[0.04em] text-muted-foreground/80">
                {group.group}
              </p>
              <ul className="space-y-0.5">
                {group.items.map((item) => {
                  const Icon = item.icon;
                  const active = tab === item.key;
                  return (
                    <li key={item.key}>
                      <button
                        type="button"
                        onClick={() => setTab(item.key)}
                        aria-current={active ? "page" : undefined}
                        className={cn(
                          "flex w-full items-center gap-2 rounded-lg px-2 py-[7px] text-left text-[12.5px] transition-colors duration-150",
                          active
                            ? "bg-accent font-medium text-accent-foreground"
                            : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                        )}
                      >
                        <Icon
                          className={cn(
                            "size-4 shrink-0",
                            active ? "text-primary" : "text-muted-foreground",
                          )}
                        />
                        <span className="min-w-0 flex-1 truncate">{item.label}</span>
                        {item.key === "ignored" && ignored.length > 0 && (
                          <span className="shrink-0 text-[10.5px] tabular-nums text-muted-foreground/70">
                            {ignored.length}
                          </span>
                        )}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </nav>

        {/* 右侧内容：左对齐、限宽 760，不用居中——居中的表单在宽窗口里像漂着 */}
        <main className="min-h-0 flex-1 overflow-y-auto">
          <div className="w-full max-w-[760px] px-8 py-7">
            <div className="mb-5">
              <h2 className="text-[15px] font-semibold tracking-tight">
                {meta.title}
              </h2>
              <p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground">
                {meta.desc}
              </p>
            </div>
            {renderContent()}
          </div>
        </main>
      </div>

      <PathPickerDialog
        open={pickerOpen}
        mode="folder"
        title="选择游戏库根目录"
        onOpenChange={setPickerOpen}
        onPick={(path) => void confirmRoot(path)}
      />
    </div>
  );
}
