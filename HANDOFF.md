# HANDOFF.md — 项目状态交接

> 写给"下一个接手的人/下一个对话的 AI"。
> 先读这份，再读 `DECISIONS.md`（决策理由）。两者冲突时以本文件的**现状**为准、以 DECISIONS 的**理由**为准。
> 最后更新：2026-10-17 19:34

---

## 0. 一句话现状

A 阶段收尾：**静态主界面已完成并通过截图验收，动效已换成 motion 并调过手感，但还没有任何数据库、没有真实数据、git 也还没初始化。**

- 项目路径：`E:\deskbox\galgame-manager`
- 启动：`cd E:\deskbox\galgame-manager && npm run tauri dev`
- 构建：`npm run build`（tsc + vite，当前零错误）
- 产品负责人：不懂代码，按"每步停下等确认"的方式协作，验收靠点击界面，不看代码

---

## 1. A 阶段完成情况

| 子任务 | 状态 | 说明 |
|---|---|---|
| 环境检查 | ✅ | Node 22.22.2 / Rust 1.98.0 (msvc) / MSVC BuildTools 2026 / Windows SDK 10.0.26100 / WebView2 155 |
| 旧数据探查（只读） | ✅ | 见 `E:\deskbox\legacy-data-report.md`（项目外，15 KB） |
| 数据模型定稿 | ✅ | 见下方第 4 节 |
| 项目初始化 | ✅ | Tauri v2 + React + TS + Vite + Tailwind v4 + shadcn(radix-nova) |
| 亮色设计 token | ✅ | 落在 `src/index.css` 的 `:root` |
| 静态三栏主界面 | ✅ | 12 条中文假数据，四态可切换，键盘导航可用 |
| 动效升级 motion | ✅ | 入场/悬停/按压/选中/布局重排全部换 motion |
| 桌面窗口打开验证 | ✅ | 1440×900，产品负责人已确认"窗口 OK" |

### 明确留下的尾巴（**都还没做**）

1. **详情面板切换游戏时没有内容过渡**——仍是瞬间替换，需要 `AnimatePresence mode="wait"`。
2. **侧栏、详情面板本身没有包成 `motion.div`**——目前只有卡片/网格有动效。
3. **左下角调试面板展开/收起仍是瞬间切换**——需要弹簧缩放。
4. **调试浮层遮挡问题**——固定在左下角，在列表视图会盖住左侧几行的缩略图（不是 bug，但难看；产品负责人已知晓，方案未定）。
5. **`git` 尚未初始化**——原计划"第 6 步 git 保护点"没执行。首次提交前应先落一个 `.gitignore` 检查。
6. **真实数据一律没有**——`src/data/games.ts` 是手写的 12 条假数据。

---

## 2. 目录结构（截至 2026-10-17）

```
galgame-manager/
├── DECISIONS.md                  # 架构/视觉/交互决策 + 踩坑记录（必读）
├── HANDOFF.md                    # 本文件
├── README.md                     # 模板自带，未维护
├── components.json               # shadcn 配置（style: radix-nova, iconLibrary: lucide）
├── index.html                    # <title>MoeShelf Next</title>，lang=zh-CN
├── package.json
├── tsconfig.json                 # paths: @/* → ./src/*（禁止写 baseUrl）
├── tsconfig.node.json
├── vite.config.ts                # react + tailwindcss 插件；alias 用 import.meta.dirname
│
├── src/
│   ├── main.tsx                  # 挂载 + import "./index.css"
│   ├── index.css                 # ★ design token 唯一来源（:root 变量）+ 滚动条/焦点环
│   ├── App.tsx                   # ★ 全局状态编排（搜索/筛选/选中/编辑/键盘）
│   ├── vite-env.d.ts
│   │
│   ├── types/
│   │   └── game.ts               # ★ 数据模型（与未来 Rust 返回结构一对一）
│   ├── data/
│   │   └── games.ts              # 12 条假数据 + ALL_TAGS（B 阶段整体替换）
│   │
│   ├── lib/
│   │   ├── motion.ts             # ★ 动效参数唯一来源（弹簧/错峰/位移）
│   │   └── utils.ts              # shadcn 生成（注意：nova 组件实际用 npm 包 "cn"）
│   │
│   └── components/
│       ├── ui/                   # shadcn 生成的 17 个组件，**不要手改**
│       │   ├── badge / button / card / checkbox / dialog / dropdown-menu
│       │   ├── input / label / scroll-area / select / separator / skeleton
│       │   └── sonner / textarea / toggle / toggle-group / tooltip
│       │
│       ├── layout/
│       │   ├── TopBar.tsx        # 搜索 + Grid/List 切换 + 扫描 + 设置
│       │   └── FilterSidebar.tsx # 240px：平台/引擎/标签 + 清除筛选
│       │
│       ├── game/
│       │   ├── GameItem.tsx      # ★ motion.button：layout="position" + 悬停/按压/选中框
│       │   ├── GameCard.tsx      # 竖版 2:3 封面（纯视觉层）
│       │   ├── GameRow.tsx       # 列表行（纯视觉层）
│       │   ├── GameGrid.tsx      # ★ 四态 + grid/list + 量列数（ResizeObserver）
│       │   └── CoverArt.tsx      # SVG 假封面（poster / wide 两变体）
│       │
│       ├── detail/
│       │   └── DetailPanel.tsx   # 320px：只读 / 编辑 / 锁定小锁
│       │
│       └── debug/
│           └── StateSwitcher.tsx # 左下角浮层，开发期专用（删一行即可移除）
│
└── src-tauri/
    ├── Cargo.toml                # edition 2024；tauri 2 / serde 1
    ├── tauri.conf.json           # productName "MoeShelf Next"，1440×900，theme Light
    ├── build.rs
    ├── .cargo/config.toml        # ★ 项目局部 sparse 索引（不要删，见第 6 节坑 5）
    ├── capabilities/default.json
    ├── icons/
    └── src/
        ├── main.rs
        └── lib.rs                # ⚠️ 只有一个模板自带的 greet 占位命令，B 阶段在这里加
```

**motion 相关新增文件（最近一轮）**：`src/lib/motion.ts`、`src/components/game/GameItem.tsx`。

---

## 3. 依赖版本

### 前端（package.json 声明 → 实际安装）

| 包 | 声明 | 实装 |
|---|---|---|
| react / react-dom | ^19.1.0 | **19.3.1** |
| motion | ^14.3.0 | **14.3.0** |
| vite | ^8.0.16 | **8.3.4** |
| typescript | ~6.0.3 | **6.0.3** |
| tailwindcss / @tailwindcss/vite | ^4.3.3 | **4.3.3** |
| radix-ui（shadcn 汇总包） | ^1.8.1 | 1.8.1 |
| lucide-react | ^1.59.0 | 1.59.0 |
| sonner | ^2.0.8 | 2.0.8 |
| @tauri-apps/api | ^2 | 2.12.4 |
| @tauri-apps/cli（dev） | ^2 | 2.12.3 |
| @fontsource-variable/geist | ^5.3.0 | 5.3.0 |
| class-variance-authority / clsx / tailwind-merge | 0.7.1 / 2.1.1 / 3.7.0 | 同 |
| cn（shadcn nova 组件实际 import 的包） | ^0.5.0 | 0.5.0 |
| tw-animate-css | ^1.4.0 | 1.4.0 |
| shadcn（CLI 被当作依赖装进来了） | ^4.22.0 | 4.22.0 |

> ⚠️ **不要删除 `cn` 包**。shadcn 的 nova 预设让组件 `import { cn } from "cn"`，删了整站编译失败。

### Rust

| 项 | 版本 |
|---|---|
| edition | 2024 |
| tauri | 2.12.4（spec "2"） |
| tauri-build | 2.7.3 |
| tauri-plugin-opener | 2 |
| serde / serde_json | 1 |
| rustc / cargo | 1.98.0（`x86_64-pc-windows-msvc`） |

**当前打包体积**：前端 JS 约 608 KB（gzip 194 KB），motion 占约 140 KB。

---

## 4. 数据模型（已与产品负责人逐条确认，B 阶段照此建表）

`src/types/game.ts` 里的 `Game`：

```
id            // 主键 = 8 位 hex 的路径哈希（djb2），沿用旧库，封面缓存靠它
pathHash      // 同上（历史原因两个都留；B 阶段可合并）
title         // 罗马字/英文
titleCn       // 中文：officialCnTitle 优先，无则 cnTitle
originalTitle // 日文原名
developer
released      // YYYY-MM-DD
engine        // kirikiri | siglus | artemis | bgi | renpy | unity —— 必须独立成列（左侧筛选要用）
platform      // windows | android（为 G 阶段预留）
tags[]        // 来自 bangumi genres
rating / ratingSource   // 只留主评分 + 来源（vndb | bangumi）
description
launchExe     // 主程序（旧库 exeMap 提升成列）
folderPath / fileCount / matchScore
completed
firstSeenAt   // 首次入库，重扫不覆盖
lockedFields[]// ★ 元数据锁定，替代旧库 7 张 Map
vndbId?
```

**已废弃、禁止复活**：`title/title_cn/developer/cover_url` 作为"运行时镜像列"的设计（旧库这四列 211 条真实数据填充率 0%）。

---

## 5. 视觉与动效规范（改之前先看这里）

### 颜色（`src/index.css` 的 `:root`，组件里禁止硬编码）

| 角色 | 值 |
|---|---|
| 页面底 | `#FAFAF9`（暖灰白，非纯白） |
| 卡片/面板 | `#FFFFFF` |
| 正文 | `#27272A`（非纯黑） |
| 次要文字 | `#71717A` |
| 强调 | `#4F46E5`（indigo-600，只给关键动作） |
| 浅强调底 | `#EEF2FF` |
| 边框 | `#E7E5E4` |
| 圆角 | `0.75rem`（12px） |

对比度全部 ≥ 4.5:1（实测 14.26 / 4.63 / 6.29 / 8.88）。

### 布局

顶栏 56px ｜ 左栏 240px ｜ 右栏 320px ｜ 中间自适应（`minmax(140px, 1fr)`，1440 宽下 5 列）。
滚动只发生在中间区和详情面板内部，`body` 是 `overflow: hidden`。

### 动效（`src/lib/motion.ts`，参数只改这一个文件）

| 场景 | 参数 |
|---|---|
| 入场 | `opacity 0→1`、`y 20→0`、错峰 **45ms**、`spring(300,24)` |
| 悬停 | `scale 1.04`、`y -4`、`spring(400,25)` |
| 按压 | `scale 0.98`、`spring(600,30)` |
| 选中框 | `scale 0.95→1`、`spring(520,28)` |
| 布局重排 | `layout="position"` + `spring(600,33)`（ζ≈0.67，超调 1.58px，290ms 落定） |

**铁律**：动效只动 `opacity` / `transform`；`useReducedMotion()` 命中时位移归零。

---

## 6. 踩过的坑与修复（按复现概率排序）

| # | 现象 | 根因 | 修复 |
|---|---|---|---|
| 1 | 卡片入场动画完全不播 | `<AnimatePresence initial={false}>` 会禁用子元素**首次**动画 | 去掉该属性 |
| 2 | 网格↔列表切换时卡片先炸到 800px 再瘪回 | 两套视觉层交叉淡化，旧海报按新宽度维持 2:3，把容器撑高 | 视觉层直接切换，不交叉淡化 |
| 3 | 切视图像橡皮筋拉伸 | 裸 `layout` 会插值**尺寸**（152→823px），封面被同步拉长 | 改 `layout="position"`，只平移，尺寸瞬间切换 |
| 4 | 调弹簧"越调越怪" | 手感由阻尼比决定：`ζ = damping/(2√stiffness)`，只改一个数会同时改速度和幅度 | 先定 stiffness 定速度，再按 `damping = 2·ζ·√stiffness` 反算；ζ 取 0.65~0.7 |
| 5 | `cargo` 拉包报 `Could not connect ... over proxy` | 运行环境里 `http_proxy/https_proxy=127.0.0.1:4355` 不可达；且全局 `~/.cargo/config.toml` 用清华 **git** 索引 | 命令前加 `env -u HTTPS_PROXY -u HTTP_PROXY -u https_proxy -u http_proxy`；本项目 `src-tauri/.cargo/config.toml` 局部改 **sparse** 索引（不要删） |
| 6 | 卡片悬停/选中时顶边被切平 | 滚动区只有 `pb` 没有 `pt`，`overflow` 裁剪边界是 padding box | 滚动区加 `pt-6`；判据：`card.top - scroller.top ≥ 10px` |
| 7 | `tsc` 报 `TS5101 baseUrl is deprecated` | TS 6 废弃 `baseUrl` | tsconfig 只写 `paths`，不写 `baseUrl` |
| 8 | Vite 8 警告 `__dirname` | ESM 下 `__dirname` 不推荐 | `vite.config.ts` 用 `import.meta.dirname` |
| 9 | sonner 编译失败（`next-themes`） | shadcn 的 sonner 默认读 `next-themes` | 删依赖，`theme` 固定 `"light"` |
| 10 | shadcn 组件集体找不到 `cn` | nova 预设从 npm 包 `"cn"` 导入，不是 `@/lib/utils` | 保留 `cn` 依赖 |

> 排查工具：项目里有一套用 `playwright-core` + 无头 Edge 的验收脚本，位于
> `E:\deskbox\.workbuddy\tmp\pw\`（`qa-motion.js` / `qa-bounce.js` / `qa-clip.js`）。
> 用法：`msedge --headless=new --remote-debugging-port=93xx` 后 `chromium.connectOverCDP`。
> **必须带 `env -u HTTP_PROXY ...`**，否则连 localhost 会被代理拦成 502。

---

## 7. B 阶段开发计划（下一步要干的事）

目标：**让界面读真数据**，把 `SEED_GAMES` 换成 `invoke("list_games")`，并把旧库 211 条迁进来、复用已下载的封面。

### B1. 数据层落地（Rust）

1. `Cargo.toml` 加 `rusqlite = { version = "0.3x", features = ["bundled"] }`（`bundled` 避免依赖本机 sqlite）。
2. 新建 `src-tauri/src/db/`：
   - `mod.rs`：连接管理、`PRAGMA journal_mode=WAL`、`PRAGMA user_version`
   - `schema.rs`：建表 SQL（`games` / `playtime` / `scan_roots` / `game_sources`）
   - `migrate.rs`：`user_version` 版本迁移（替代旧库的 `migration_log` 表）
3. 数据目录决策（**需先问产品负责人**）：`%APPDATA%\com.moeshelf.next\` 还是项目内 `data/`？
   建议 APPDATA，避免 Program Files 写权限问题。

### B2. 旧数据迁移（一次性，只读旧库）

源：`E:\moeshelf\moeshelf-build-2.1.0\data\moeshelf.db`（211 games / 58 playtime / 372 scrape_cache）

规则（已与产品负责人确认）：
- `path_hash` **原样搬运**（保住 885 MB 封面缓存的可用性）
- 跳过 4 条测试夹具（`folder_path` 含 `AppData\Local\Temp\moeshelf-scan-fixture`）
- `scrape_cache` 里 `success=0` 的 7 条**只搬 key 不搬 data**
- 元数据从 `scrape_cache.data_json` 反查补进新列（`released` / `engine` / `tags` / `rating` …）
- `engine` 从 `matched_types` 的 `engine:xxx` 提取成独立列
- 人工修改（旧 `settings.json` 的 7 张 `*Map`）→ 写进 `locked_fields` + 对应列
- 迁移脚本必须**只读打开旧库**（`mode=ro`），且先在副本上跑通

### B3. Tauri 命令

先只做三个，够界面跑起来：

```rust
#[tauri::command] fn list_games() -> Result<Vec<Game>, String>
#[tauri::command] fn get_game(id: String) -> Result<Option<Game>, String>
#[tauri::command] fn update_game(id: String, patch: GamePatch) -> Result<(), String>
```

`Game` 结构体加 `#[serde(rename_all = "camelCase")]`，与 `src/types/game.ts` 一一对应，前端不做二次映射。

### B4. 前端接真数据

1. `App.tsx`：`useState(SEED_GAMES)` → `useEffect` 里 `invoke<Game[]>("list_games")`，保留 loading 状态。
2. `DetailPanel` 保存 → `invoke("update_game")`，成功后本地合并（保留现有 Toast 文案）。
3. `CoverArt` → 真封面：`convertFileSrc(cachePath)` 指向旧 `cache/images/{hash}.bin`，
   或把缓存目录整体复制到新数据目录（**产品负责人要求复用，不许重新下载**）。
4. `ALL_TAGS` → 由数据动态汇总。

### B5. 封面对接方案（二选一，需产品负责人拍板）

- 方案 A（推荐）：读取时直接用旧路径 `E:\moeshelf\moeshelf-build-2.1.0\data\cache\images\`，零拷贝但依赖旧目录存在。
- 方案 B：迁移时把 885 MB 复制/移动进新数据目录，之后可删旧项目。

### B6. B 阶段验收标准

- [ ] 冷启动窗口里显示 **211 部**真实游戏，标题/会社/发售日/引擎/标签齐全
- [ ] 封面能显示（不是渐变占位图）
- [ ] 左侧筛选的引擎/标签计数与真实数据一致
- [ ] 改一个字段保存 → 关掉应用重开 → 修改仍在，且带小锁
- [ ] 旧库文件时间戳**未被修改**（`moeshelf.db` 仍是 2026-10-08 22:16）
- [ ] `npm run build` 零错误

---

## 8. 不许破坏的架构底线

1. **Tauri v2 承载一切系统能力**。扫描/启动/进程跟踪/文件写入都在 Rust 命令层；前端不直接碰文件系统。
2. **禁止 Electron**、**禁止 Node 后端进程**、**禁止 localhost 页面形态**。
3. **禁止引入状态管理库和路由**（motion 是唯一例外，已记录理由）。
4. **`src/components/ui/` 不手改**（shadcn 生成物）；要改视觉就改 `src/index.css` 的变量。
5. **颜色只走 CSS 变量**，组件里不写死 hex。
6. **中文界面**，所有面向用户的文案用中文。
7. 旧数据**只读**。任何写旧库的操作都是事故。

---

## 9. 与产品负责人的协作方式（重要）

- 他不懂代码，**每一步做完要停下来等确认**，不要一口气改十个文件。
- 报错时：**先一句话说根因**，再给修复步骤；禁止连续猜测式修改。
- 每步结束给"验收清单"，用**点击哪里、看到什么**来描述，不要写技术术语。
- 他关注视觉手感，会直接说"太挤/太弹/太艳"，按他的话调参数即可，参数都在 `motion.ts` 和 `index.css`。
