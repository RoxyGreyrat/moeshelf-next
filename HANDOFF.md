# HANDOFF.md — 项目状态交接

> 写给"下一个接手的人/下一个对话的 AI"。
> 先读这份，再读 `DECISIONS.md`（决策理由）。两者冲突时以本文件的**现状**为准、以 DECISIONS 的**理由**为准。
> 最后更新：2026-10-19 15:40（启动器完成：首次确认启动程序 + 计时 + 启动次数）

---

## 0. 一句话现状

**C 阶段已收口**：211 部真实数据在 SQLite 里、封面走 `cover://` 直读旧缓存（211/211 命中）、
编辑保存真落库并锁定字段、211 部下切视图 4 帧 → 46 帧；目录扫描器（Rust 规则引擎）与
**前端完整接线**（选目录 → 实时进度 → 写库 → 刷新列表 → 报「新增/更新」）都已跑通；
扫描收进来的工具目录可以**逐个忽略**（设置里能恢复）或**删除记录**；
**扫描后自动匹配资料**（VNDB / 月幕，标题完全一致才写）已接上，设置里可关；
**中文简介 / 角色声优**可以按需补全（月幕 → Bangumi，角色按名字对上才写）；
**体积已按真实目录补统计**（209/212，共 1.06 TB；剩 3 部目录已被改名/删除）；
**界面状态全部记忆**（视图 / 排序 / 筛选 / 搜索词 / 选中项 / 筛选栏收起 / 分组折叠）；
**启动器已完成**（首次确认启动程序 → 真起进程 → 计时 → 记启动次数与游玩时长）。
**现在只剩打磨：文案/动效统一梳理、多选批处理、启动器的进程树跟踪。**

2026-10-19 补的四块：
1. **封面候选不再搜索** —— 直接读 `games.vndb_id`（211 部里 204 部有），取该条目主封面
   + 所有 release 的包装图；没关联条目时界面给引导，不发请求（详见 DECISIONS 15）；
2. **设置页新增「网络与代理」** —— 可手填 HTTP 代理（Clash 的 `http://127.0.0.1:7890`）
   并逐个测试 VNDB / Bangumi / 月幕连通性（详见 DECISIONS 16）；
3. **补丁管理落地** —— 预览会动哪些文件 → 备份 → 覆盖 → 一键还原；支持 zip / 文件夹 /
   安装器 exe；`patch_scan` 扫出库里 113 个疑似补丁目录（详见 DECISIONS 17）；
4. **扫描后自动匹配资料** —— 新入库的游戏按标题去 VNDB / 月幕找，完全一致才写入，
   已人工改过的跳过；设置 → 刮削与匹配 可关（详见 DECISIONS 18）。

- 项目路径：`E:\deskbox\galgame-manager`
- 启动：`cd E:\deskbox\galgame-manager && npm run tauri dev`
- 构建：`npm run build`（tsc + vite，零错误）
- Rust 测试：`cd src-tauri; $env:HTTPS_PROXY=$null; $env:HTTP_PROXY=$null; cargo test`（**86 passed / 7 ignored**）
  > 这台机器上系统是 **Windows PowerShell 5.1**，没有 `env` 命令，别照抄 `env -u ...`；用 `$env:XXX=$null`。
  > 也别用 PowerShell 的 `Get-Content -Raw` 改中文 md：5.1 会按 ANSI 读、写回去就是乱码（已踩过一次）。
- 数据库：`src-tauri\data\moeshelf.db`（WAL 模式，schema v5，9 张表，211 行）
- git：最新提交 `d2f8924`（返回列表同步数据 + 禁用整页重载）

### 2026-10-19 用户实测反馈（处理状态）

| # | 反馈 | 状态 |
|---|---|---|
| 1 | 从详情页返回列表，手动改过的东西不显示，要刷新 | ✅ 已修（`d2f8924`）：`onClose` 时回刷该行 |
| 2 | F5 / Ctrl+R 会整页重载（像浏览器） | ✅ 已修（`d2f8924`）：拦下改成"重新读取游戏库" |
| 3 | **选择补丁后没有任何反应** | ✅ 已处理（2026-10-19）：zip / 文件夹两条路径都真机跑通，未复现"完全没反应"；问题大概率是**只有 Toast、弹窗里一片安静**——已补：读取中横幅、失败内联错误、候选条目直接「预览」。详见 7.9 |
| 4 | **导入新游戏不会自动匹配资料** | ✅ 已实现（2026-10-19）：扫描完自动按标题匹配 VNDB / 月幕，完全一致才写入。详见 7.10 |

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

**D3. 补丁管理** —— ✅ 已完成 2026-10-19（commit `7385a27`）
- 预览（不动硬盘）→ 备份 → 覆盖 → 一键还原；支持 zip / 文件夹 / 安装器 exe
- rar/7z 明确提示"先解压再选文件夹"；超过 200MB 的文件不备份，预览里标红
- 实现与取舍详见 DECISIONS 第 17 节

**风险**：UAC（管理员 exe）、含空格/日文的路径、需要兼容层的老游戏，这三类会在 D1 集中出现。

---

### 7.8 本机环境的两个坑（接手时先看）

1. 这台机器的 shell 是 **Windows PowerShell 5.1**，不是 PowerShell 7：
   - `env -u XXX cargo ...` 会报「无法将 env 项识别为 cmdlet」→ 改用 `$env:HTTP_PROXY=$null`
   - `.ps1` 文件含中文时必须存 **UTF-8 with BOM**，否则按 GBK 解析、直接语法错误
2. **沙箱限制会让 `pwsh` 起不来**（退出码 0xC0000142，DLL 初始化失败）。
   本会话切到「完全权限」后正常。若再遇到，先确认权限模式，不要在沙箱里反复重试。

---

### 7.9 补丁安装：这轮补的反馈（2026-10-19）

**先说结论**：zip 与文件夹两条导入路径都真机跑通了，**没能复现"完全没反应"**。
但代码里确实有一处"看起来像没反应"的真空：**读取补丁期间，弹窗主体完全静止**，
只有 Toast 和右上角按钮转圈；读失败时同样只有一闪而过的 Toast。补了四处：

| 改动 | 位置 | 解决什么 |
|---|---|---|
| 候选补丁条目加「预览」按钮 | `PatchManagerDialog` 候选列表 | 以前只有「打开位置」，点条目本身没反应 —— 用户以为选中了 |
| 读取中横幅 | 弹窗正文顶部 | 大压缩包解压几十秒时，界面不再静止 |
| 读取失败改为正文内联错误 + 「换一个补丁」 | 同上 | 不再依赖一闪而过的 Toast |
| `choosePatch` 先关选择器、再切视图 | 同上 | 双弹窗叠着的时候状态更明确 |

**端到端实测（真机、真文件）**

```
预览：新增 1 / 覆盖 0（zzz_patch_probe.txt 57B）
应用：记录 status=applied，文件真的出现在游戏目录
还原：记录 status=reverted，文件被删掉
清理：patch_delete 后记录消失，游戏目录恢复原状
```

⚠️ 验收脚本用 Playwright 的 `getByRole(..., { name: "补丁" })` 时**必须加 `exact: true`**：
默认是子串匹配，`补丁` 会命中预览页的「应用这个补丁」—— 我第一次就是这样误应用了一次。
误应用的探针文件与记录已还原并清理（`restored 0 / removed 2`）。

### 7.9.1 补丁包格式：全部常见格式都能直接选（2026-10-19）

| 格式 | 支持 | 实现 |
|---|---|---|
| `.zip` | ✅ | `zip` crate，文件名 UTF-8 失败时按 GBK 兜底 |
| `.7z` | ✅ | `sevenz-rust2`（纯 Rust） |
| `.rar` | ✅ | `unrar`（官方源码，RAR4 / RAR5 都支持） |
| `.tar` / `.tar.gz`(`.tgz`) / `.tar.bz2`(`.tbz2`) / `.tar.xz`(`.txz`) | ✅ | `tar` + `flate2` / `bzip2` / `lzma-rust2`（纯 Rust） |
| `.exe` | ✅ | 当安装器处理（快照对比），不是解压 |
| 加密压缩包 | ⚠️ | 明确提示"有密码，请先解压"（没有输密码的界面） |
| 分卷包 `.001/.002`、`.rar`（多卷） | ❌ | 提示不认识的格式 / 解压失败 |

安全约束（三种解压实现都遵守）：
- 条目路径一律过 `join_rel`：`../`、绝对路径直接跳过，不会写到暂存目录外；
- tar 只接受普通文件和目录，符号链接/设备文件一律跳过（防止解出链接再写出包外）；
- 解压失败会删掉半个包，不在暂存目录里留垃圾。

**真机实测（用户库里真实存在的补丁包，只做预览、全部未应用）**

```
.rar  「生命的备件 汉化补丁.rar」      加密包 → 「这个压缩包有密码，请先用解压软件解开」
.rar  「八卦恋爱汉化补丁1.0.5.rar」   1 个文件 · 新增 1
.7z   「牛顿与苹果树 汉化补丁.7z」    353 个文件 · 覆盖 353
.zip  「euphoriahdr_rn.zip」         3 个文件 · 覆盖 3
.tar.gz / .tar.bz2 / .tar.xz（临时构造）各 2 个文件 · 新增 2
```

> 预览会在 `data/patches/staging/<id>` 留一份解压结果，2 小时过期自动清；
> 实测 7z 预览占地 390MB，这些测试产物已手动清空。


### 7.10 扫描后自动匹配资料（2026-10-19）

**规则：只有标题归一化后完全相等才写库。**

- 归一化（`scrape::normalize_title`）：全角转半角 → 去掉括号内容 → 只留字母/数字/假名/汉字 →
  去掉「汉化版 / 中文版 / 免安装 / 绿色版 …」这类发布形态词 → 小写。
- 打分（`scrape::title_score`）：与候选的 **主标题 / 原文名 / 中文名** 任意一个完全相等 = 1.0；
  只包含（短串 ≥4 字且占比 ≥60%）= 0.6~0.9，**只用于提示，永不写入**。
- 源顺序：**VNDB → 月幕**。Bangumi 不参与：它的搜索是模糊的（搜「王様恋愛」第一条是
  「王様物語」），不满足"完全相等"的前提。
- 已经有人工修改（`locked_fields` 非空）的条目不碰。
- 开关：设置 → 刮削与匹配 → 「扫描完成后自动补全资料」，默认**开**，
  存在 `app_settings` 的 `scrape.autoMatchOnImport`。

**链路**：`scan_directory` 返回 `insertedIds` → 前端 `autoMatchGames(ids)` →
Rust 逐条 搜索 → 详情 → 下载封面 → 复用 `apply_candidate_fields`（与手动「获取资料」同一套合并规则）
→ 每条发 `auto-match-progress` → 状态行显示「正在匹配资料 3/18」→ 完成后一句 Toast：
「自动补全 N 部资料 · M 部没找到确定匹配」。

**真机实测**

```
扫描测试目录（2 个文件夹）
→ toast：扫描完成，新增 2 部 / … · 自动补全 1 部资料 · 1 部没找到确定匹配
→ 王様恋愛 → 国王恋爱 / ASa Project / 2026-09-25 / 评分 7.4 / vndb v63828 / 封面已下载
→ "zzz 不存在的游戏 98765" → 没有搜到同名条目（不乱猜）
清理测试行后回到 212 部
```

**代价与后续**：完全相等意味着"文件夹名带了一堆广告词"的游戏会漏掉。
以后要放宽，只能改成"按候选的评分/发售日交叉验证后写入"，不能简单降阈值 ——
自动写入一次会改 6~8 个字段，认错一部就得手工改回来。

---

### 7.11 左侧筛选（Wallpaper Engine 式）+ 主列表排序（2026-10-19）

**筛选面板**：`src/components/layout/FilterSidebar.tsx`

| 能力 | 说明 |
|---|---|
| 分组 | 会社 / 引擎 / 标签 / 平台，每组可折叠，标题右侧是可选值总数 |
| 数量 | 每行尾部是该值下的条目数；数量为 0 的行变淡但仍可点 |
| 搜索 | 面板顶部搜会社 / 引擎 / 标签；有搜索词时**强制展开**所有分组并只显示匹配项 |
| 折叠策略 | 每组默认露 8 行 + 「显示全部 N 个」；标签组（630 个）默认收起，不会把侧栏撑爆 |
| 选中置顶 | 已选中的值排在该组最上面，选了几个之后不用再翻几百行去找它 |
| 平台 | 默认收起（真实库里只有 Windows） |

**排序**：`src/lib/sort.ts` + 顶栏右侧的「字段下拉 + 方向按钮」

| 字段 | 空值处理 |
|---|---|
| 发售日（默认，倒序） | 没有发售日的永远沉底 |
| 名称 | 用**显示出来的标题**（中文 → 外文 → 文件夹名）做 `localeCompare` 拼音比较，数字按数值 |
| 评分 | 数值比较，同分按名称 |
| 入库时间 | ISO 字符串比较 |

- 方向和字段拆成两个控件：换字段时方向不会被重置；
- 排序结果同时用于**键盘导航**（方向键的"下一部"和屏幕上的顺序一致），
  实现在 `App.tsx` 的 `sortedGames`；
- 空值沉底与正/倒序无关，避免"没有发售日的游戏在倒序时跑到最前面"。

**实测（真机、真库 212 部）**

```
分组：会社 85 · 引擎 10 · 标签 630 · 平台 1
点「Yuzusoft」→ 共 212 部 → 共 13 部
面板搜索「柚子」→ 只剩标签组，5 行（柚子社 13 / 柚子奈ひよ 4 / 柚子 1 / 柚子处女作 1 / 柚子奈 1）
排序：发售日倒序 → 国王恋爱 2026 / 心 2026 / 命运石之门：重启 2026
      名称正序   → 9-nine- 新章 / 9-nine-春色… / 9-nine-九次…
      发售日正序 → AIR 2000 / 沦为回忆的你 2002 / 秋之回忆2 2003
```

### 7.12 补全中文简介 / 角色声优（2026-10-19）

VNDB 有角色但**没有声优**、简介是英文；旧项目这两样是从月幕 / Bangumi 补的。

- 命令：`enrich_game(id)` → `EnrichReport { descriptionSource, cvFilled, characters, notes }`
- 顺序：**月幕 → Bangumi**，标题精确匹配（复用 `scrape::title_score`），找不到就不动
- 月幕角色要二次展开：`?gid=` 只给 `{cid, cvId, position}` → 每个 `?cid=` 拿名字/立绘 →
  每个 `?pid=` 拿声优（同一部作品里 CV 共享，加了内存缓存；请求间 120ms，避免限流）
- 角色合并按**归一化名字**（去空格/标点、小写）匹配，别名也参与；已有角色只补空着的 `cv`/立绘，
  不覆盖已有的
- 入口：详情页顶部「补全资料」按钮；**自动匹配成功后也会自动跑一次**
- 源 id 记在 `game_sources(source_id)` 里，第二次补全不用再搜

**真机实测（国王恋爱 / v63828，2026-09-25 新作）**

```
补全前：中文简介 0 字 · 角色 0
补全后：中文简介 248 字（月幕）· 角色 10 · 有声优 9（月幕）
样例：むぎ / 桜咲 千依、佐藤 緋華 / 三宅麻理恵、高宮 来葉 / あさみ ほとり
```

> 月幕的角色/人物接口返回结构是 `{ data: { character | person } }`，
> 第一版按顶层字段取，导致"角色 0 个"——已修正并留了排查用例：
> `cargo test ymgal_real_characters -- --ignored --nocapture`。

### 7.13 体积补统计（2026-10-19）

旧库没有体积列，迁移过来的 211 部 `dir_size` 全是 0（详情页显示「未统计」）。

- 命令：`measure_sizes(only_missing)` → `SizeReport { measured, totalBytes, missingBefore, missingDirs }`
- 递归统计（上限与扫描器一致），每 150ms 发一次 `size-progress`
- 入口：设置 → 存储与缓存 → 「游戏体积」两个按钮（统计未统计的 / 全部重新统计）
- **实测：209 部已统计，合计 1059.6 GB**；剩 3 部目录已不存在（见下）

```
0bd95484 告别回忆 -无垢少女- 致最爱的你   E:\galgame\5pb\Memories Off8\...  → 目录不存在
8b7abc17 时廻者+                         E:\galgame\Key\LOOPERS PLUS       → 目录不存在
cd8e8c54 Girls Frantic Clan              E:\galgame\戯画\ガールズ...        → 目录不存在
```

这三条是早期扫描发现的「文件夹被改名/移动」的残留，统计时如实报 `missingDirs`，
不会把它们算成 0 字节。要不要清理交给用户（详情页可以「删除」）。

### 7.14 界面状态记忆（2026-10-19）

"关掉重开，看到的东西和上次一样"：`src/lib/persist.ts`

| 记住什么 | 存在哪 |
|---|---|
| 视图（网格/列表）、排序字段+方向、筛选、搜索词、选中项、光标 | `localStorage["moeshelf.ui.v1"]` |
| 筛选栏收起状态 | `localStorage["moeshelf.sidebarCollapsed"]` |
| 每个筛选分组的展开/收起 | `localStorage["moeshelf.filterGroups.v1"]` |

- 只存"用户的选择"，不存游戏数据（数据以库为准）
- 读出来先过 `sanitizeUi`：手改/版本升级留下的脏值一律回退默认，不让界面崩
- 写入防抖 250ms，拖筛选时不会狂写
- **修了一个隐蔽 bug**：`reload()` 原来无条件清空选中，导致启动时"恢复选中"立刻被清掉。
  现在改成"只剔掉已不存在的 id"，启动/扫描/删除都能保住仍有效的选中。

### 7.15 筛选栏可收起 + 排序口径（2026-10-19）

- 侧栏右上角一个按钮收起成 44px 窄条（竖排「筛选」+ 展开按钮 + 已选数量徽标），状态记本地
- **会社按名称排**（不再按数量）：找"我知道的那家公司"时，数量排序帮不上忙
- 引擎数量少且没有自然顺序，仍按数量从多到少；都保持"已选中置顶"
- 详情页封面：高度固定 264px、宽度按图片实际比例（横版宣传图不再被裁），竖版仍是 176×264

> 2026-10-19 追加：**「标签」筛选分组已整组删除**（产品负责人明确要求）。
> 现在侧栏只有 会社 / 引擎 / 平台；筛选搜索框的占位文案同步改成「搜索会社 / 引擎」。
> 顶栏的全局搜索仍然会搜标签内容（那是搜索，不是筛选）。
> 类型 `Filters` 也去掉了 `tags` 字段，持久化里的旧 `tags` 会被 `sanitizeUi` 丢弃。

### 7.16 启动器（2026-10-19）

**能日常用的最后一块补上了。** 现在点「启动游戏」是真起进程，并记录启动次数与游玩时长。

| 能力 | 说明 |
|---|---|
| 启动 | `launch_game(id, exe?)`，工作目录 = 游戏目录；已在运行时直接返回 pid，不会开出第二个 |
| 首次确认 | 启动项未锁定（`locked_fields` 里没有 `launchExe`）时，第一次点会弹出**选择启动程序**面板：列出目录里的 exe（扫描器排序）+「手动选择其他程序」 |
| 记住选择 | 选过之后写回 `launch_exe` **并加锁** —— 这就是"只问一次"的实现；以后重扫也不会改 |
| 更改启动项 | 面板里随时可以改（运行中也可以，下次启动生效） |
| 计时 | 进程退出后回填 `playtime_sessions.ended_at/seconds`，累加 `playtime.minutes` |
| 启动次数 | `playtime.sessions`，在**进程真的起来之后**才 +1（启动失败不算） |
| 结束游戏 | `stop_game` 走 `taskkill /T /F`，连子进程一起杀 |
| 退出通知 | `game-exited { gameId, seconds }`，前端收到后刷新计时/时长 |

**UI**：`components/launch/LaunchControl.tsx`

- 侧栏详情：面板弹在**按钮上方**（产品要求）；详情页顶栏：面板向下弹（上方没空间），宽 340、右对齐
- 动画：`springPanel`（420/32），`transformOrigin` 贴着按钮一侧，进出都是缩放+位移
- 三种状态：选择启动程序 / 正在游玩（计时 + 结束游戏）/ 游戏已退出（可再启动一次）
- 时长也出现在详情页：游玩时长、启动次数、上次游玩

**实测（真机）**

```
测试行（目录里一个 TestGame.exe）：
  首次点启动 → 面板出现「选择启动程序」，候选 1 个 → 选中后进程起来
  运行中 running=true，计时 1s → 4s
  结束游戏 → running=false，minutes 从 0 变成 0.0666（4 秒）
  启动次数 sessions 正确累加（3 次：两次误测 + 一次真实）
真实游戏（国王恋爱）：首次点启动 → 面板列出 王様恋愛.exe / ファィル破損チェックツール.exe
```

**已知取舍**：进程跟踪只看**直接子进程**。有些 galgame 是"启动器 → 正片"两层，
父进程退出后子进程可能还在，这一版的时长会按父进程算短。要更准得上 Job Object 或轮询进程树。

### 7.17 接口整理（2026-10-19）

- `get_game_detail` 一次返回 **游戏 + 角色 + 游玩记录**（原来要拼三次，详情页会出现"时长晚半秒才到"的跳变）
- 游玩记录另有 `get_playtime` 单独命令：运行中每秒刷新 / 退出后刷新用，不必重拉整页
- `launch_game` 成功时顺手把确认过的启动项写库并加锁，省掉一次 `update_game` 往返
- 启动候选 `list_exe_candidates` 与启动分开：面板可以反复打开、重试，不必重复读目录

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
