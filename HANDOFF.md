# HANDOFF.md — 项目状态交接

> 写给"下一个接手的人/下一个对话的 AI"。
> 先读这份，再读 `DECISIONS.md`（决策理由）。两者冲突时以本文件的**现状**为准、以 DECISIONS 的**理由**为准。
> 最后更新：2026-10-19 13:40（封面候选改为读已关联条目 + 应用内代理设置）

---

## 0. 一句话现状

**C 阶段已收口**：211 部真实数据在 SQLite 里、封面走 `cover://` 直读旧缓存（211/211 命中）、
编辑保存真落库并锁定字段、211 部下切视图 4 帧 → 46 帧；目录扫描器（Rust 规则引擎）与
**前端完整接线**（选目录 → 实时进度 → 写库 → 刷新列表 → 报「新增/更新」）都已跑通；
扫描收进来的工具目录可以**逐个忽略**（设置里能恢复）或**删除记录**。
**离"能日常用"只差：启动器（启动游戏 + 计时）、补丁管理。**

2026-10-19 补的两处：
1. **封面候选不再搜索** —— 直接读 `games.vndb_id`（211 部里 204 部有），取该条目主封面
   + 所有 release 的包装图；没关联条目时界面给引导，不发请求（详见 DECISIONS 15）；
2. **设置页新增「网络与代理」** —— 可手填 HTTP 代理（Clash 的 `http://127.0.0.1:7890`）
   并逐个测试 VNDB / Bangumi / 月幕连通性（详见 DECISIONS 16）。

- 项目路径：`E:\deskbox\galgame-manager`
- 启动：`cd E:\deskbox\galgame-manager && npm run tauri dev`
- 构建：`npm run build`（tsc + vite，零错误）
- Rust 测试：`cd src-tauri; $env:HTTPS_PROXY=$null; $env:HTTP_PROXY=$null; cargo test`（68 passed / 5 ignored）
  > 这台机器上系统是 **Windows PowerShell 5.1**，没有 `env` 命令，别照抄 `env -u ...`；用 `$env:XXX=$null`。
- 数据库：`src-tauri\data\moeshelf.db`（WAL 模式，schema v4，6 张表，211 行）
- git：见 `git log --oneline`（最新一次提交是「C 阶段前端接线」）

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

### B 阶段 Step 1 新增（2026-10-17 19:45）

| 子任务 | 状态 | 说明 |
|---|---|---|
| git 初始化 + 首次提交 | ✅ | `199353f`，75 文件，无 node_modules/target/db 泄漏 |
| rusqlite 接入 | ✅ | 0.40.2，`features=["bundled"]`（自带 SQLite 源码，不依赖 dll） |
| 建表 `init_db` | ✅ | games(33 列) / playtime / playtime_sessions / scan_roots / game_sources，7 个索引，`user_version=1` |
| 数据库位置 | ✅ | `src-tauri/data/moeshelf.db`（开发期）；打包后走 `%APPDATA%\com.moeshelf.next\data` |
| `list_games` / `add_dummy_game` | ✅ | 命令层薄封装，SQL 在 `db/repo.rs`，有 2 个单测覆盖 |
| 前端接 `list_games` | ✅ | 假数据文件 `src/data/games.ts` 已删除，标签由数据动态汇总 |
| 旧数据迁移 | ⬜ | 下一步 |

### B 阶段 Step 2：旧数据迁移（2026-10-17 20:00）

| 子任务 | 状态 | 说明 |
|---|---|---|
| 迁移模块 | ✅ | `src-tauri/src/db/migrate.rs`，快照 → 只读打开 → 事务内 INSERT OR REPLACE |
| 触发方式 | ✅ | 左下角调试面板「迁移旧库（一次性）」按钮 → `migrate_legacy` 命令 |
| 实测结果 | ✅ | 扫描 211 / 导入 211 / 有元数据 211 / 人工覆盖 186 处 / 时长 56 条 |
| 演示数据清理 | ✅ | 迁移前自动删掉 4 条 `add_dummy_game` 的行，避免混进真实库 |
| 引擎兜底 | ✅ | 出现 `catsystem2`，`Engine` 类型补登记 + `engineLabel()` 取不到就原样显示 |
| 标题兜底 | ✅ | 中文名 → 外文名 → 文件夹名 三级，保证卡片不会空标题 |

**触发方式**：跑 `npm run tauri dev` → 点左下角「状态调试」里的**迁移旧库（一次性）** → 等 1 秒 → Toast 报数字。
迁移**不**在启动时自动跑，必须手动点，避免每次开应用都覆盖手工修改。

### 明确留下的尾巴

1. ~~详情面板切换游戏时没有内容过渡~~ → ✅ 2026-10-19 加了 0.18s 淡入（key=game.id，**不做 exit 等待**：
   连续按方向键时"先淡出再淡入"会出现空白帧）。
2. **侧栏本身没有包成 `motion.div`**——详情面板已有淡入；左筛选栏仍是静态的（影响很小）。
3. ~~左下角调试面板展开/收起瞬间切换~~ → ✅ 2026-10-19 改为弹簧（spring 420/32）。
4. ~~调试浮层遮挡~~ → ✅ 2026-10-19 **默认收起**成一枚圆形按钮，不再压住列表左侧。
5. ~~编辑保存还没落库~~ → ✅ 2026-10-17 已实现 `update_game`，编辑→保存→刷新后仍在。
6. ~~封面还是渐变占位~~ → ✅ 2026-10-17 已接 `cover://` 协议，211/211 命中旧缓存。
7. ~~`add_dummy_game` 与"插入测试数据"按钮是开发期脚手架~~ → ✅ 2026-10-17 已删除。
8. **`engine_probe` 的指纹识别还没接进正式流程**——目前只在迁移时补了 31 部，unknown 仍有 80 部。
9. **标签编辑是"文本串回填"**——UI 上没有增删胶囊的交互，且多词标签依赖 `、` 分隔。长期应改成 chip 编辑器。
10. **筛选与选中的一致性** → ✅ 2026-10-19 修：筛选把选中项藏起来时自动清空选中（之前会出现
    左边"共 0 部"、右边还挂着上一部游戏）。

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
│   │   └── game.ts               # ★ 数据模型（与 Rust 返回结构一对一）
│   │
│   ├── lib/
│   │   ├── api.ts                # ★ 所有 invoke() 的唯一出口（listGames / addDummyGame）
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
    ├── Cargo.toml                # edition 2024；tauri 2 / serde 1 / rusqlite 0.40.2(bundled)
    ├── tauri.conf.json           # productName "MoeShelf Next"，1440×900，theme Light
    ├── build.rs
    ├── .cargo/config.toml        # ★ 项目局部 sparse 索引（不要删，见第 6 节坑 5）
    ├── capabilities/default.json
    ├── icons/
    ├── data/                     # 开发期数据库（.gitignore 已忽略）
    │   └── moeshelf.db
    └── src/
        ├── main.rs
        ├── lib.rs                # 启动时 db::init + manage + 注册命令
        ├── commands.rs           # ★ 命令层（薄壳：只做 State 取用与错误转换）
        ├── models.rs             # ★ Game 结构体（serde camelCase）+ from_row
        └── db/
            ├── mod.rs            # 数据目录解析、open_at、schema 版本升级
            ├── schema.rs         # ★ 建表 SQL（V1_DDL）+ SCHEMA_VERSION
            └── repo.rs           # ★ SQL 实现：list_all / insert_demo_games / count_all + 单测
```

**最近两轮新增**：`src/lib/motion.ts`、`src/components/game/GameItem.tsx`（动效）；`src/lib/api.ts`、`src-tauri/src/{commands,models}.rs`、`src-tauri/src/db/*`（数据层）。

---

## 2.5 数据库现状（B1 交付物）

**位置**：开发期 `src-tauri\data\moeshelf.db`；打包后 `%APPDATA%\com.moeshelf.next\data\moeshelf.db`。
**已验证**：`user_version=1`、`journal_mode=wal`、5 张表、`games` 33 列、7 个 `idx_*` 索引。

| 表 | 作用 |
|---|---|
| `games` | 主表。`id` 就是 8 位 path_hash，`engine` / `platform` 是独立列，`tags` / `locked_fields` 存 JSON 文本 |
| `playtime` | 汇总（minutes / sessions / last_played），逻辑同旧库 |
| `playtime_sessions` | 新增：按会话记账，旧库只有一个累计值 |
| `scan_roots` | 新增：多根目录 + 各自开关/排除项，替代旧库单值 `rootPath` |
| `game_sources` | 新增：多源原始 payload，某源挂了不丢数据 |

**命令**（前端唯一入口 `src/lib/api.ts`）：

```rust
list_games()                     -> Vec<Game>    // 按 first_seen_at DESC 排序，过滤 ignored=1
update_game(id, patch, lock)     -> Game         // 增量补丁 + 锁定字段，返回整行
list_ignored_games()             -> Vec<Game>    // 忽略清单（设置里显示，可恢复）
set_games_ignored(ids, ignored)  -> usize        // 忽略 / 取消忽略，整批一个事务
delete_games(ids)                -> usize        // 删记录 + 连带 playtime/会话/来源
pick_folder(title?)              -> Option<String> // 原生选目录，取消返回 null
scan_directory(root)             -> ScanReport   // 扫描 + 写库，过程发 scan-progress 事件
migrate_legacy(legacy_dir?)      -> MigrateReport // 运维兜底，界面无入口
```

⚠️ 脚手架（`add_dummy_game`、"插入测试数据"按钮）已在 `ca02f4b` 清理掉。

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
> `E:\deskbox\.workbuddy\tmp\pw\`（`qa-motion.js` / `qa-clip.js` / `qa-scan.js` …）。
> 两种用法：
> 1. **纯界面**：`msedge --headless=new --remote-debugging-port=93xx` 打开 `http://localhost:1420`，
>    用 `page.addInitScript` 伪造 `window.__TAURI_INTERNALS__.invoke`（见 `qa-scan.js`）。
> 2. **真 IPC**：给窗口加 `additionalBrowserArgs: "--remote-debugging-port=9222"` 再启动：
>    `npx tauri dev --config E:\deskbox\.workbuddy\tmp\tauri.cdp.json`
>    （配置放项目外，别提交）。然后 `chromium.connectOverCDP("http://127.0.0.1:9222")`，
>    可以 `page.evaluate(() => __TAURI_INTERNALS__.invoke("scan_directory", {root}))` 直接打真命令。
> **必须带 `env -u HTTP_PROXY ...`**（PowerShell 下是 `$env:HTTP_PROXY=$null`），否则连 localhost 会被代理拦成 502。

---

## 7. B 阶段开发计划（B1–B4 已完成，剩 B5 可选加分项）

目标：**把旧库 211 条真实数据迁进来，复用 885 MB 封面缓存，编辑能落库。**

### B1. 数据层落地（Rust）—— ✅ 已完成 2026-10-17

- `rusqlite 0.40.2` + `features = ["bundled"]`
- `src-tauri/src/db/{mod,schema,repo}.rs` 分工：连接/迁移、建表 SQL、SQL 实现
- 数据目录：开发期 `src-tauri/data/`（编译期 `CARGO_MANIFEST_DIR`），打包后 `%APPDATA%\com.moeshelf.next\data`
- 命令：`list_games` / `add_dummy_game`（薄壳），2 个单测覆盖建表与幂等插入
- 前端：`src/lib/api.ts` 统一出口，`App.tsx` 启动时读库，假数据文件已删除
- 验证方式：跑应用看控制台 `[db] 就绪：...（schema v1）`；空库时界面显示空态，点"插入 4 条测试数据"后出现卡片

### B2. 旧数据迁移（一次性，只读旧库）—— ✅ 已完成 2026-10-17

实现：`src-tauri/src/db/migrate.rs`（`migrate::run(&conn, legacy_dir)`）。

实测（`cargo test migrate_real_legacy -- --ignored --nocapture`）：

```
games_scanned  211   skipped_fixture 0（2.1.0 活库里本来就没有夹具）
imported       211   with_metadata   211   without_metadata 0
playtime       56    overrides_applied 186
引擎分布  unknown 111 / kirikiri 55 / bgi 18 / artemis 16 / siglus 10 / catsystem2 1
评分范围  3.8 ~ 8.8（已从 0~100 归一到 0~10），仅 1 部无评分
锁定字段  coverUrl 68 / titleCn 62 / launchExe 37 / developer 12 / title 7
```

关键实现细节（**改这个文件前必读**）：
- 旧库 WAL 模式不能直接只读打开 → 先把 `db/-wal/-shm` 复制到 `%TEMP%\moeshelf-legacy-snapshot`，再 READ_ONLY 打开副本
- 关联键：`scrape_cache.cache_key LIKE '%::' || games.path_hash`（实测 211/211 命中），必须排除 `company::%`
- `rating` 旧库是 0~100，新模型 0~10，除以 10 并保留 1 位小数
- `exe_candidates` 在 2.1.0 是小写 `name/path`，旧 C# 库是大写 `Name/Path`，两种都兼容
- `titleMap` 的值可能是中文也可能是英文 → 含 CJK 进 `title_cn`，否则进 `title`，都记进 `locked_fields`
- 事务前先 `purge_demo_games()`，否则演示数据会混进真实库

### B2 原始计划（已完成，保留备查）

源：`E:\moeshelf\moeshelf-build-2.1.0\data\moeshelf.db`（211 games / 58 playtime / 372 scrape_cache）

规则（已与产品负责人确认）：
- `path_hash` **原样搬运**（保住 885 MB 封面缓存的可用性）
- 跳过 4 条测试夹具（`folder_path` 含 `AppData\Local\Temp\moeshelf-scan-fixture`）
- `scrape_cache` 里 `success=0` 的 7 条**只搬 key 不搬 data**
- 元数据从 `scrape_cache.data_json` 反查补进新列（`released` / `engine` / `tags` / `rating` …）
- `engine` 从 `matched_types` 的 `engine:xxx` 提取成独立列
- 人工修改（旧 `settings.json` 的 7 张 `*Map`）→ 写进 `locked_fields` + 对应列
- 迁移脚本必须**只读打开旧库**（`mode=ro`），且先在副本上跑通

### B3. Tauri 命令 —— ✅ 已完成 2026-10-17

```rust
list_games()                     -> Vec<Game>   // 已实现
get_game(id)                     -> Option<Game> // 已实现为 repo::get_one（未暴露成命令，暂无调用方）
update_game(id, patch, lock)     -> Game         // ✅ 已实现
```

`update_game` 的设计要点（**改之前必读**）：

- 入参 `patch: GamePatch` 的**每个字段都是 `Option`，缺省 = 不改这一列**。
  前端只传改动过的字段，因此不会把 `first_seen_at`、`folder_path` 这类字段覆盖成空。
- `lock` 默认 true：把本次改动的列名并进 `locked_fields`（去重），这就是界面上的小锁。
  记录的字段名是 **camelCase**（`titleCn` / `coverUrl`），与旧库迁移过来的数据保持一致。
- 列名只在 `repo.rs` 的 `LOCK_NAMES` 白名单里取，SQL 的列名永远不来自外部输入。
- 写完 `updated_at = strftime(...)`，然后**重新 SELECT 整行返回**。
  前端拿返回值替换本地那一条，不整表重拉 —— 整表重拉会清空选中，详情面板会跟着关掉。
- 校验在 `GamePatch::validate()`：中文标题不可为空、发售日必须 `YYYY-MM-DD`（空串允许）、评分 0~10。

命令参数名刻意都用**单个英文单词**（`id` / `patch` / `lock`），避开 Tauri 参数名 camelCase↔snake_case 转换的不确定性。

### B4. 前端接真数据 —— ✅ 已完成

1. ✅ `App.tsx`：启动 `useEffect` → `list_games`，保留 loading/错误态。
2. ✅ `DetailPanel` 保存 → `update_game`，成功后用返回行替换本地那一条；保存期间按钮转圈并禁用。
   保存失败**保持编辑态**，不吞用户输入。
3. ✅ `CoverArt` → 真封面：`cover://` 自定义协议直读旧缓存（方案 A，零拷贝），失败回落 CSS 渐变。
4. ✅ 标签由数据动态汇总（`ALL_TAGS` 已删）。

### B5. 封面对接 —— ✅ 方案 A 已落地，方案 B 仍可选

- 方案 A（**已采用**）：`cover://<8位hex>` 协议读 `E:\moeshelf\moeshelf-build-2.1.0\data\cache\images\{hash}.bin`，
  按旁挂 `.json` 里的 contentType 设响应头。零拷贝、零下载，211/211 命中。
  **代价：旧目录不能删。**
- 方案 B（⬜ 未做，可选加分项）：写一个"搬移封面缓存"命令，把 885 MB 复制进新数据目录，之后可安全删除旧项目。
  注意是**按需复制**（只搬库里 cover_url 命中的那 211 个文件，约 60 MB），不是整目录 10,349 个文件。

### B6. B 阶段验收标准 —— 进度

- [x] 冷启动窗口里显示 **211 部**真实游戏，标题/会社/发售日/引擎/标签齐全
- [x] 封面能显示（不是渐变占位图）
- [x] 左侧筛选的引擎/标签计数与真实数据一致
- [x] 改一个字段保存 → 刷新 / 关掉重开 → 修改仍在，且带小锁
- [x] 旧库文件时间戳**未被修改**（`moeshelf.db` 仍是 2026-10-08 22:16）
- [x] `npm run build` 零错误

---

## 7.5 C 阶段：目录扫描器（已完成 2026-10-17）

**能力**：`scan_directory(root)` → 递归识别游戏目录 → 写库（新增 or 只刷新扫描事实）→ 边扫边发 `scan-progress` 事件。

实现位置：
- `src-tauri/src/db/scanner.rs` —— 全部规则（打分 / 实锤 / 容器 / 候选排序 / 统计）
- `src-tauri/src/db/repo.rs::upsert_scanned` —— 幂等写库，受 `locked_fields` 保护
- `src-tauri/src/commands.rs::scan_directory` —— `spawn_blocking` + 事务

规则来源：旧版 `E:\moeshelf\moeshelf-src-latest\src\lib\scan.ts`（935 行），逐条移植并在注释里标了「为什么」。

**真机实测（`E:\galgame`，只扫描不写库）**

```
扫 379 个目录 / 105.9s，识别 225 部
与已迁移的 211 部比对：207 对上 / 18 新增 / 4 未扫到
引擎  unknown 90 · kirikiri 61 · bgi 17 · artemis 16 · unity 13
      siglus 12 · yuris 8 · renpy 4 · catsystem2 3 · advhd 1
主程序判定比旧库更准：26 部
```

4 条「未扫到」的真相：3 条是**文件夹已改名/移动**（`Key\LOOPERS PLUS` → `LOOPERS` 等），
扫描已用新路径重新认出；1 条 `smee\PureConnectFullHD` 是布局差异（见下方已知问题）。

**已知问题（下一轮处理）**

1. 18 条新增里约 6 条是工具目录：`GARbro`、`GARbro-Mod-1.0.1.2.0`、`FreeMoteToolkit`、
   `lz4_win32_v1_9_3`、`Zcode`。它们本身带 `.xp3`/数据文件，规则上"像游戏"。
   → **现在可以手动忽略**（详情面板底部「忽略」，见 7.6.1）；自动识别工具目录仍未做。
2. 容器布局取舍：`PureConnectFullHD\PURExCONNECT\game.exe` 这种，
   旧库把**父目录**当游戏，新扫描器把**子目录**当游戏 → 重扫会多出一条。
   当前选择保留子目录（更符合真实作品名），代价是少量重复。
3. `unknown` 仍有 90 部（旧库 80）。扫描器的引擎判定比迁移时的 `engine_probe` 更保守，
   且 `engine_probe` 在扫描路径里只在命中时才被调用。

**下一步**：曾经是「前端接 `scan_directory`」，现已完成 —— 见 7.6。

### 7.6 C 阶段前端接线（2026-10-17 22:00 完成）

| 改动 | 位置 | 说明 |
|---|---|---|
| 原生选目录 | `commands.rs::pick_folder` | `tauri-plugin-dialog` 的**回调 API + oneshot**，不是 `blocking_pick_folder()`。只用 Rust API，capabilities 里**不需要** `dialog:*` 权限 |
| 前端出口 | `src/lib/api.ts` | 新增 `pickFolder` / `scanDirectory` / `onScanProgress`；`invoke` 仍然只在这一个文件里出现 |
| 扫描流程 | `App.tsx::handleScan` | 选目录 → 先订阅再调用 → 扫完 `reload()` → Toast 报数字；用户取消就静默返回 |
| 按钮状态 | `TopBar.tsx` | 扫描中按钮禁用 + 转圈 + 文案变「扫描中…」 |
| 进度显示 | `layout/ScanStatus.tsx` | 嵌在书架状态行里，**临时替换右侧快捷键提示**（不做浮层：浮层会盖住封面、插横条会让网格上下跳） |

**实测（真机、真 IPC、真库）**

```
scan_directory 返回 { dirsScanned: 3, gamesFound: 2, inserted: 2, updated: 0, elapsedMs: 119 }
F5 之后：状态行 共 211 部 → 共 213 部，新增的两部排在最前
清理测试行后重新 F5：回到 211 部
```

UI 流程（按钮 → 进度 → Toast → 刷新）用无头 Edge + 假 `__TAURI_INTERNALS__.invoke` 跑过一遍：
`正在扫描 188 个文件夹 · 发现 4 部 · E:\galgame\...` → Toast「扫描完成，新增 2 部 / 识别 225 部 · 更新 209 部」。
脚本在 `E:\deskbox\.workbuddy\tmp\pw\qa-scan.js`、`qa-real-scan.js`。

**C 阶段还没做的**（按优先级）：
1. **取消扫描**：现在点开始就只能等（`E:\galgame` 实测 90~105 秒）。Rust 侧需要一个 `AtomicBool` 取消标志。

> 2026-10-19 补充：**扫描根目录已落地**（`scan_roots` CRUD + 顶栏扫描下拉 + 设置页管理），
> 扫过的目录会自动登记，不用每次翻磁盘。**打开文件夹**也做了（详情面板底部的路径可点、
> 设置页可打开数据目录/封面缓存）。原先这两条尾巴已关闭。

### 7.6.1 忽略 / 删除（同日完成）

入口在**详情面板底部那行**：`忽略`（左）/ `删除`（右），多选时文案变成「忽略这 N 部」。

| 动作 | 语义 | 后端 |
|---|---|---|
| 忽略 | `games.ignored = 1`，书架过滤掉，**行还在**；重扫不会把它放出来（`upsert_scanned` 不碰 ignored） | `set_games_ignored(ids, true)` |
| 删除 | 真删行 + 连带 `playtime` / `playtime_sessions` / `game_sources`。**不动硬盘文件、不删封面缓存** | `delete_games(ids)` |
| 恢复 | 设置 → 已忽略 → 「恢复」 | `set_games_ignored([id], false)` |

- 删除必须二次确认，确认文案里写明「不动硬盘文件、重扫会回来、只是不想看见请用忽略」
- 两个动作都是**整批一个事务**，不会出现"删了一半"
- 设置面板 `components/settings/SettingsDialog.tsx`，目前只有「已忽略」一块

**实测（真机、真 IPC、真库，用一条合成行 `deadbeef` 走完整流程）**

```
初始 212（211 真实 + 1 条测试）
忽略 → 211，卡片消失，忽略清单 ['deadbeef']
设置 → 1 条，含测试条目 → 恢复 → 212，卡片回来
删除 → 确认弹窗「从书架删除这一部？」→ 删除 → 211，库里查不到该行，忽略清单 0 条
```

### 7.7 下一步：D 阶段（启动器 + 补丁管理）建议顺序

**D1. 启动游戏（最小可用）**
- 命令 `launch_game(id)`：按 `games.launch_exe` 起进程，工作目录用 `folder_path`
- 启动前校验 exe 是否存在：不存在就把错误原样抛给界面，**不要静默失败**
- 会话记账：`playtime_sessions` 插一行 start_at；退出后回填 end_at / duration 并累加 `playtime`
- 进程跟踪：`wait()` 必须放后台线程（不能阻塞 IPC），退出事件用 `emit` 回前端
  ⚠️ galgame 常见"启动器再拉子进程"，只 `wait()` 父进程会提前记结束 —— 需要 Job Object 或轮询进程名
- 前端：详情面板「启动游戏」按钮接上；`App.tsx` 里的 `playing` 现在还是 1.4 秒假定时器

**D2. 玩过多久**
- 详情面板显示 总时长 / 最近游玩 / 次数；列表加"按最近游玩排序"

**D3. 补丁管理**
- 先做"发现 + 列出"，不自动应用：扫描时把 `patch/`、`汉化补丁/` 之类记进库
- 真要应用就必须**先快照被覆盖的文件**到 `%APPDATA%\com.moeshelf.next\patch-backup\`，没有还原能力就不提供一键

**风险**：UAC（管理员 exe）、含空格/日文的路径、需要兼容层的老游戏，这三类会在 D1 集中出现。

---

### 7.8 本机环境的两个坑（接手时先看）

1. 这台机器的 shell 是 **Windows PowerShell 5.1**，不是 PowerShell 7：
   - `env -u XXX cargo ...` 会报「无法将 env 项识别为 cmdlet」→ 改用 `$env:HTTP_PROXY=$null`
   - `.ps1` 文件含中文时必须存 **UTF-8 with BOM**，否则按 GBK 解析、直接语法错误
2. **沙箱限制会让 `pwsh` 起不来**（退出码 0xC0000142，DLL 初始化失败）。
   本会话切到「完全权限」后正常。若再遇到，先确认权限模式，不要在沙箱里反复重试。

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
