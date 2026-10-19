# DECISIONS.md — 架构与视觉决策记录

> 记录"为什么这么做"，而不是"做了什么"。后来的人（包括三个月后的自己）看这份就能接手。
> 最近更新：2026-10-17 · 第 4 步（静态主界面）

---

## 0. 一句话

Windows 本地 Galgame 管理器：**Tauri v2 承载全部系统能力，React 只做界面**，界面是"书架"而不是"后台管理表格"。

---

## 1. 架构决策

### 1.1 为什么是 Tauri v2，不是 Electron

| 方案 | 否决原因 |
|---|---|
| Electron | 打包 100 MB+、内存占用高、Chromium 自带一份等于重复造轮子 |
| 旧 MoeShelf（Node 服务端 + 浏览器访问 localhost） | 暴露端口、需要额外进程守护、用户要手动开浏览器，本质是"伪装成桌面应用的网页" |
| **Tauri v2** | 用系统自带 WebView2（Win10/11 默认存在），安装包小一个数量级；Rust 侧能直接做进程树跟踪、文件系统、注册表 |

**这是硬约束**：项目里不允许出现 `localhost` 页面形态、不允许 Node 后端服务。

### 1.2 系统能力全部下沉 Rust 命令层

前端**永远不直接碰文件系统**。后续所有需要 OS 权限的动作都走 `#[tauri::command]`：

```
扫描目录 / 读数据库 / 启动游戏 / 跟踪进程树 / 写补丁 / 快照回滚
        ↕  invoke("cmd_name", args)
        React 组件（只负责展示与收集意图）
```

好处：权限边界清晰（capabilities 白名单可审计）、前端可整体换掉而不动业务逻辑、将来 Android 端能复用同一套命令语义。

**当前状态**：`src-tauri/src/lib.rs` 里只有模板自带的 `greet` 占位，第 B 阶段开始往这里加命令。

### 1.3 前端为什么"不引入状态库、不引入路由"

本期只有一个界面、一份假数据。引入 Redux/Zustand/React Router 只会增加理解成本。
跨组件状态用 `App.tsx` 的 `useState` 提升解决（详见第 5 节）。
等真正出现"多页面 + 深层嵌套"（大概率是 D 阶段刮削面板），再评估是否引入，届时在这里补决策。

> **唯一的例外：`motion`（Framer Motion 14）。**
> 它不提供状态管理和路由，只负责动画。当"手感"成为硬需求（弹簧、错峰、共享布局动画），
> CSS transition 已经表达不出来——尤其"网格↔列表原地变形"必须靠 motion 的 `layout` 投影。
> 代价是打包体积增加约 140 KB（gzip 后 47 KB），桌面应用可忽略。

### 1.4 目录约定

```
src/
  types/game.ts          # 数据模型（与 Rust 返回结构一对一）
  data/games.ts          # 假数据（B 阶段换成 invoke，不删文件，便于回退）
  components/ui/         # shadcn 生成，不手改（改样式走 index.css 变量）
  components/layout/     # TopBar / FilterSidebar
  components/game/       # GameGrid / GameCard / GameRow / CoverArt
  components/detail/     # DetailPanel
  components/debug/      # StateSwitcher（仅开发期存在）
src-tauri/.cargo/config.toml  # 项目局部用的 sparse 索引（见第 7 节）
```

**规则**：`components/ui/` 是第三方生成的代码，业务定制一律写在外层组件，避免以后 `shadcn add` 覆盖时冲突。

---

## 2. 数据模型决策

完整分析见 `E:\deskbox\legacy-data-report.md`。核心结论：

### 2.1 彻底废弃"运行时镜像列"

旧库 `games` 表的 `title / title_cn / developer / cover_url` 四列在 211 条真实数据里**填充率 0%**——它们只是刮削缓存的投影，查询时由 API 层临时补值。结果是"任何一处忘了补值，界面就显示文件夹名"。

**新模型：元数据直接落真列。** 缺数据就是 `NULL`，用 `displayTitle` 一处兜底（`title_cn || title || folder_name`），不在查询路径上做隐式回退。

### 2.2 沿用 / 提升 / 新增 / 丢弃

| 类别 | 内容 |
|---|---|
| **沿用** | `path_hash`（djb2，8 位 hex）、`folder_path/name`、`exe_candidates`、`match_score`、`matched_types`、`file_count`、`first_seen_at` |
| **提升为列** | `title`、`title_cn`、`original_title`、`developer`、`released`、`cover_url`、`description`、`cn_description`、`rating` + `rating_source`、`tags`、**`engine`**、**`launch_exe`**、`vndb_id`、`bangumi_id` |
| **新增** | **`platform`**（为 Android 预留）、`locked_fields`（JSON）、`field_sources`（JSON）、`manual_match`、`ignored`；`scan_roots` 表替代单值 `rootPath`；`game_sources` 表存多源原始记录 |
| **丢弃** | 镜像列设计、`settings.json` 里的 7 张 `*Map`、明文 API Key、`migration_log` 表（改 `PRAGMA user_version`）、`rootPath` 单值、`recycle_bin.snapshot` 整段 JSON |

### 2.3 两个关键决定

1. **`engine` 必须是独立列**，不能藏在 `matched_types` 的 JSON 数组里。
   原因：左侧筛选栏要按引擎分组计数，JSON 里翻等于全表扫描 + 前端过滤。
   来源：扫描时从 `engine:kirikiri|siglus|artemis|bgi|renpy` 提取。

2. **`locked_fields` 用 JSON 数组替代 7 张 Map。**
   旧版把人工修改散在 `exeMap / coverMap / titleMap / devMap / steamAppIdMap / cvMap / bigscreenAssetsMap`，键都是 `path_hash`——想知道"这一部的标题是自动的还是人工改的"要翻 7 个地方，而且没有锁定时间、无法做界面小锁。
   新模型一个字段解决：`["titleCn","developer"]`，界面读它决定是否显示小锁。

### 2.4 封面缓存复用（产品负责人已拍板）

沿用同一套 djb2 路径哈希 → 直接读旧的 `data\cache\images\{hash}.bin`，**零下载继承 10,349 张 / 885 MB 封面**。
真库接入时只需把 `CoverArt` 换成 `<img src={coverUrl}>` 或 `convertFileSrc(本地缓存路径)`，外层比例、遮罩、hover 全部不用改。

---

## 3. 视觉决策

### 3.1 方向：编辑式极简（Editorial Minimal），不是"后台管理系统"

**拒绝的提案**：`ui-ux-pro-max` 引擎按 "gaming" 关键词推导出「霓虹紫 #CE1BDA + Anton 字体 + 发光描边」。
拒绝理由：那套是"游戏电竞风"，与"干净、现代、不廉价"直接冲突——霓虹色在亮色底上最显廉价，Anton 这类展示字体在中文界面里根本落不了地（无中文字形）。
**采纳的**：只保留它的 P0 可访问性门禁（对比度、键盘可达、焦点环、触摸目标）。

最终方向：**大留白 + 克制的信息密度 + 单一强调色**。差异化的记忆点放在**封面本身**（低饱和双色渐变 + 宋体大字水印 + 印刷斜纹），而不是装饰性的 UI 元素。

### 3.2 色彩（全部写在 `src/index.css` 的 `:root`，组件里禁止硬编码）

| 角色 | 值 | 用途 |
|---|---|---|
| `--background` | `#FAFAF9` | 页面底，暖灰白（不用纯白，避免刺眼） |
| `--card` | `#FFFFFF` | 卡片 / 面板 |
| `--foreground` | `#27272A` | 正文，深灰（不用纯黑，纯黑压白对比过硬） |
| `--muted-foreground` | `#71717A` | 次要文字、字段名 |
| `--primary` | `#4F46E5` | indigo-600，**只给关键动作**：主按钮、选中边框、勾选框 |
| `--accent` | `#EEF2FF` | indigo-50，hover 底、选中行底 |
| `--border` | `#E7E5E4` | 极浅暖灰边框 |
| `--radius` | `0.75rem` | 12px，全局统一 |

实测对比度（WCAG AA）：

| 组合 | 比值 | 结论 |
|---|---|---|
| 正文 #27272A / 页面 #FAFAF9 | 14.26:1 | ✅ |
| 次要文字 #71717A / 页面 #FAFAF9 | 4.63:1 | ✅ |
| 白字 / 靛蓝按钮 #4F46E5 | 6.29:1 | ✅ |
| accent 文字 #3730A3 / #EEF2FF | 8.88:1 | ✅ |

### 3.3 字体

- UI：**Geist Variable**（shadcn nova 预设自带），中文字形缺失 → 必须补回退链
  `'Geist Variable', 'Microsoft YaHei UI', 'PingFang SC', 'Noto Sans SC', system-ui`
  **不补回退的话，中文会掉到系统宋体，整个界面立刻变廉价。**
- 封面水印：宋体栈（`Noto Serif SC → Songti SC → SimSun`）。只在低透明度（0.17）下使用，模拟印刷品，不承担可读性。
- 数字：`tabular-nums`，避免评分/年份跳动。

### 3.4 阴影与圆角

三级阴影，全部低透明度，避免"卡片浮起来"的廉价感：

| 层级 | 值 | 用在哪 |
|---|---|---|
| 静置 | `ring-1 ring-black/[0.07]` | 卡片默认（用 1px 描边代替阴影，更干净） |
| hover | `0 14px 30px -14px rgba(24,24,27,0.45)` | 卡片悬浮 |
| 浮层 | `0 16px 40px -16px rgba(24,24,27,0.35)` | 调试面板、Toast |

圆角：卡片 `12px`，输入框/按钮 `8px`，胶囊 `9999px`，头像类 `6px`。**不做超大圆角**（那是"玩具感"）。

### 3.5 动效（2026-10-17 升级为 motion 引擎）

> 产品负责人明确"本地 GPU 富余，不需要为性能牺牲手感"，因此从 CSS transition 全面切到 **motion（Framer Motion 14）**。所有弹簧参数集中在 `src/lib/motion.ts`，组件里不散写数值。

| 场景 | 参数 | 位置 |
|---|---|---|
| 卡片入场 | `opacity 0→1`、`y 20→0`、错峰 45ms、`spring(300, 24)` | `GameItem` |
| 悬停 | `scale 1.04`、`y -4`、`spring(400, 25)` | `GameItem` |
| 按压 | `scale 0.98`、`spring(600, 30)` | `GameItem` |
| 选中外框 | `scale 0.95→1`、`opacity 0→1`、`spring(520, 28)` | `GameItem` |
| 布局重排（网格↔列表） | `layout="position"` + `spring(600, 33)`，ζ≈0.67，超调 ≈1.5px，290ms 落定 | `GameItem` / `GameGrid` |
| 状态面板切换 | `AnimatePresence mode="wait"`，180ms | `GameGrid` |

**十个必须记住的坑**：

1. `<AnimatePresence initial={false}>` 会**禁用子元素的首次动画**。网格入场不动，就是被它吃掉的。
2. 网格↔列表**不能**给两套视觉层加交叉淡化：旧海报在动画期间会按"新的卡片宽度"继续维持 2:3 比例，容器高度瞬间冲到 800px 再缩回，观感是"炸开再瘪下去"。
   正确做法：视觉层直接切换，只让外层 `layout` 做位置变形。
3. **`layout` 必须写 `layout="position"`，不能只写 `layout`。**
   裸 `layout` 会对**尺寸**插值：152px 的卡片被横向拉到 823px 期间，封面按比例被同步拉长，看着像橡皮筋。
   `position` 让尺寸瞬间切换、只平移位置，实测高度取值集合只剩 1 个值（无任何中间高度）。
4. **布局弹簧调的是"阻尼比"，不是数值大小。**
   `ζ = damping / (2·√stiffness)`。要"有回弹但不晕"，把 ζ 定在 0.65~0.7（超调 5%）：
   - 只改 stiffness 会让速度变快、但幅度也跟着变（因为 ζ 变了）
   - 正确做法：先定 stiffness 决定"多快收住"，再按 `damping = 2·ζ·√stiffness` 反算
   - 实例：`spring(260, 22)` 幅度合适但收尾 450ms → 等比放大到 `spring(600, 33)`，幅度 1.58px 不变、落定 290ms
   - 参照：ζ≈1 等于没有回弹（`spring(250, 30)` 实测 0.0px）；ζ<0.5 才会明显晃

**渐进降级**：`useReducedMotion()` 命中时入场位移与错峰归零，动画仍在但不位移。
入场/悬停只动 `opacity` 与 `transform`，并挂 `will-change: transform`，全部走合成层。

### 3.6 规模与性能（211 部真实数据后补）

旧实现假设"库里只有十几部"，接真库后切视图直接掉到 **4 帧 / 900ms**。四层修复，按收益排序：

| 层 | 做法 | 收益 |
|---|---|---|
| 1 | **封面从 SVG 换成 CSS**：每张卡原本是一棵内联 SVG（渐变 + pattern + clipPath + 水印），211 棵 SVG 树重建+栅格化是主因；改成 CSS 渐变层 + 一张 `<img>` | 最大 |
| 2 | **分页渲染**：一次只挂 40 个真实节点，滚动到哨兵再追加 | 切视图成本降到 1/5 |
| 3 | **大列表关掉 layout 投影**：>120 条时 `layout` 传 undefined，改用整块 180ms 淡入遮掉重排 | 去掉每帧 211 个包围盒计算 |
| 4 | **切视图走 `startTransition`**：让浏览器先响应点击，再提交列表 | 点击不再"粘手" |

**实测（211 条，无头浏览器软件渲染）**

| 指标 | 修复前 | 修复后 |
|---|---|---|
| 900ms 内帧数 | 4 | 46 |
| p50 帧耗时 | 183ms | **16.7ms（60fps）** |
| 最差帧 | 233ms | 50~66ms |
| 超过 20ms 的帧 | 3~4（连续） | 2~3（单次） |

**阈值常量**在 `GameGrid.tsx` 顶部：`LAYOUT_ANIM_LIMIT=120`、`ENTRANCE_LIMIT=36`、`PAGE_SIZE=40`。
真的做到几千部时不要继续调这些常量，应该上虚拟滚动（只渲染视口内行），这一轮没做。

---

## 4. 布局决策

### 4.1 三栏固定 + 中间自适应

```
┌──────────────────────────────────────────────────────────┐
│ TopBar  h-14：搜索(左) | Grid/List | 扫描 | 设置(最右)      │
├──────────┬────────────────────────────────┬──────────────┤
│ 筛选     │ 封面网格（auto-fill 140px）     │ 详情 320px   │
│ 240px    │ 5 列 @1440 宽                   │ 固定         │
└──────────┴────────────────────────────────┴──────────────┘
```

- 左 240px / 右 320px **固定不缩放**：两侧是"工具"，中间是"内容"，内容优先拿剩余宽度。
- 网格用 `repeat(auto-fill, minmax(140px, 1fr))`：窗口变宽自动加列，不写断点。
- 页面本身 `overflow: hidden`，滚动只发生在网格区和详情面板内部（桌面应用不该出现整页滚动条）。

### 4.2 为什么选竖版 2:3 封面

- Galgame 官方包装图几乎都是竖版（VNDB 封面同样），横向裁切会丢掉构图；
- 竖版在同样宽度下能放更多部，符合"书架"心智；
- **例外**：详情面板顶部用 16:9 横图（`CoverArt variant="wide"`），充当 banner。竖图裁进横框会把水印裁成一个巨大的残字，所以 wide 变体只保留渐变与斜纹，不放水印。

### 4.3 hover 为什么是"常驻渐变 + 悬停加深"

参考图的要求是"hover 时底部渐变遮罩显示标题"。但如果标题**只在 hover 时出现**，用户扫视时满屏都是无字封面——这是画廊的做法，不是管理器的做法。

实现：底部渐变**常驻**（`from-black/80`）保证任何时候都能读到标题与会社；hover 时封面 `scale(1.045)` + 抬升 3px + 浮出评分与引擎角标。
**同一块渐变，两种强度**，既不重复标题，也不牺牲可读性。

### 4.4 选中态

- 单选：`ring-2 ring-primary ring-offset-2`（2px 靛蓝 + 2px 留白，和背景分得开）
- 多选：右上角圆形勾选标记；未选中的格子显示半透明空心圈作为"可勾选"提示
- 键盘光标（多选态下未选中的那一格）：`ring-2 ring-primary/40`，比选中态淡一档

---

## 5. 交互与状态模型

### 5.1 状态放在哪

全部提升到 `App.tsx`：

```ts
games            // 可变数据源（编辑后回写）
query            // 搜索词
view             // grid | list
gridState        // normal | loading | empty | error   ← 调试用
selectMode       // single | multi
selectedIds      // 选中集合（单选时长度为 1）
cursorId         // 键盘光标（与 selectedIds 分离，多选态必须有）
editing          // 详情面板编辑态
filters          // { platforms, engines, tags }
```

`cursorId` 与 `selectedIds` **必须分开**：多选时方向键移动的是"光标"，空格才改变"选中"，混在一起会出现"按一下方向键就多选一部"。

### 5.2 键盘映射（全局监听，输入框内自动让行）

| 键 | 行为 |
|---|---|
| `Ctrl/Cmd + K` | 聚焦搜索框并全选 |
| `← → ↑ ↓` | 网格内移动光标（单选态同步改变选中） |
| `Enter` | 打开当前光标游戏的详情 |
| `Space` | 多选态下切换当前光标游戏的选中 |
| `Esc` | 优先退出编辑态 → 再清空选中（关闭详情）→ 最后让输入框失焦 |

上下移动依赖"当前列数"，由 `GameGrid` 用 `ResizeObserver` 量第一行元素个数后回传给 `App`——不写死列数，窗口变化自动适配。

### 5.3 编辑 → 锁定流程

1. 点「编辑信息」→ 字段变输入框（`editing=true`）
2. 「保存并锁定」→ 只把**真正改动过的字段**合入 `patch`
3. `lockedFields` 取并集 → 这些字段以后刮削不会覆盖
4. 只读态在字段名旁显示小锁（标题的小锁显示在标题右侧）
5. 没有改动时点保存 → Toast「没有检测到改动」，不产生空锁定

### 5.4 调试切换器

固定在左下角（`left: 15.5rem`，避开 240px 侧栏），可折叠成圆形图标。
可切换：网格四态、单选/多选、详情只读/编辑、按钮三态（默认/禁用/加载）、成功/失败 Toast。

**已知取舍**：它是浮层，会盖住左下角一两张封面 / 一两行列表。这是有意的——它只在开发期存在，验收完可以在 `App.tsx` 里删掉一行 `<StateSwitcher />`。

---

## 6. 真数据接入点（B 阶段照着改）

| 现在 | B 阶段 |
|---|---|
| `import { SEED_GAMES }` | `await invoke<Game[]>("list_games")` |
| `CoverArt`（渐变 SVG） | `<img src={coverUrl}>` 或 `convertFileSrc(cachePath)` |
| `handlePlay` 里 `setTimeout` 假启动 | `await invoke("launch_game", { id })` |
| `handleSave` 改内存 | `await invoke("update_game", { id, patch })` |
| `handleScan` 只弹 Toast | `await invoke("scan_root", { path })` → 进度事件 → 刷新列表 |
| `ALL_TAGS` 手写 | `select distinct tag from game_tags` |

`types/game.ts` 的字段名与 Rust `serde` 结构保持一对一（`#[serde(rename_all = "camelCase")]`），前后端不做二次映射。

---

## 7. 踩过的坑（复现概率高，写下来）

1. **TypeScript 6 废弃 `baseUrl`**：`tsconfig.json` 只写 `paths: { "@/*": ["./src/*"] }`，一旦加 `baseUrl` 直接 `TS5101` 编译失败。
2. **Vite 8 对 `__dirname` 告警**：`vite.config.ts` 改用 `import.meta.dirname`。
3. **shadcn nova 预设的 `cn` 来自 npm 包 `"cn"`**，不是本地 `@/lib/utils`。删依赖会导致所有组件编译失败。
4. **cargo 拉包失败**：环境里 `http_proxy=127.0.0.1:4355` 不可达，cargo 读到就连不上。
   解法：`env -u HTTPS_PROXY -u HTTP_PROXY -u https_proxy -u http_proxy cargo ...`
   另外全局 `~/.cargo/config.toml` 用的是清华 **git** 索引（慢且易断），本项目在 `src-tauri/.cargo/config.toml` 局部改成 **sparse** 索引，不动全局配置。
5. **sonner 默认依赖 `next-themes`**：Vite 项目没有 ThemeProvider，已删依赖并把 `theme` 固定为 `"light"`。
6. **悬停/选中被滚动容器切边**：`overflow-y-auto` 的滚动区没有顶部内边距时，卡片上浮 4px + 放大 1.04（顶部再外扩约 4.5px）+ 选中外框 3px，会**超出容器 8.5px 被硬切**，圆角和边框直接消失。
   滚动区必须留 `pt-6`（24px）的呼吸空间；只给 `pb` 不够。判断方法：量 `card.getBoundingClientRect().top` 与滚动容器 `top` 的差值，静止时应 ≥ 10px。
7. **右侧详情面板"显示不全"（内容被推出窗口）**：Radix `ScrollArea` 会在 viewport 里插入一个
   `display: table; min-width: 100%` 的包裹 div。table 按 max-content 撑开，遇到长路径
   （`E:\galgame\ま〜まれぇど\…\ststeaddy.exe`）时内层涨到 **379px**，面板只有 **320px**，
   多出来的部分被推到窗口外 —— 表现就是字段值整列看不见。
   修复：`src/index.css` 里强制该包裹 div `display:block; width:100%; min-width:0`（带 `!important` 才压得住内联样式）。
   判断方法：`aside.getBoundingClientRect().width` 与 `scrollAreaViewport.firstElementChild` 的宽度是否相等。
8. **`IntersectionObserver` 在 `AnimatePresence mode="wait"` 里挂不上**：网格被延迟挂载，
   `useEffect` 执行时 `ref.current` 还是 null，而依赖又不再变化 → 观察器永远不会 attach，滚动加载失效。
   修复：改用 **callback ref**（节点出现的那一刻创建观察器），并用 `useRef` 保存 `games.length` 供回调读取。
9. **中文简介"不见了"其实不是数据问题**：迁移后 `cn_description` 填充率 211/211，
   是详情面板只渲染了英文字段 `description`。显示与编辑都改成"中文优先，没有中文才退回英文"，
   写回时也写回原来那个字段（有中文就更新 `cnDescription`，否则更新 `description`）。
10. **标签编辑按空格拆分，会把多词标签拆碎写回库（数据损坏）**：
   旧写法是 `draft.tags.split(/[、,，\s]+/)` 再和 `game.tags.join("|")` 比较。
   VNDB 的标签大量是英文短语（`White Haired Heroine` / `Science Fiction` / `Male Protagonist`），
   按 `\s` 拆会变成 `White`/`Haired`/`Heroine` 三个独立标签，然后因为 join 结果不同被判定成"用户改了"，
   直接 UPDATE 进库 —— 一次保存就永久破坏了这部游戏的标签。
   修复两点，缺一不可：
   - **先比原始字符串**（`draft.tags !== game.tags.join("、")`），没动过就一个字节都不写；
   - 分隔符只认 `、 , ，`，**绝不包含 `\s`**。
   注意：这是"文本串回填"这种编辑方式的固有风险，长期应换成 chip 增删编辑器。

---

## 8. 保存落库（`update_game`）的设计取舍

编辑器提交的是 **`GamePatch`（增量补丁）**，不是整行。理由有三条：

1. **不覆盖没碰过的字段**。整行 `UPDATE` 会把 `first_seen_at`、`folder_path`、`file_count`
   这些本轮根本没在界面上出现的列写回空值 —— 一次编辑就能把扫描结果抹掉。
2. **锁定的语义天然对齐**。"改过的字段才锁定"，补丁的 key 集合就是锁定名单，不需要前端再传一遍。
3. **SQL 可以只 `SET` 真正变动的列**，其余列连写都不写。

命令返回**更新后重新 `SELECT` 出来的整行**，前端用它替换本地那一条，**不整表重拉**。
理由：`reload()` 会清空 `selectedIds` 与 `cursorId`，保存完详情面板会直接关掉，用户以为出错了。
"真的落盘"这件事由两条来保证：Rust 侧 `update_survives_reopen` 单测（重开连接读回），
和前端 F5 / 关掉重开（重新走 `list_games`）。

其它约定：
- `locked_fields` 里记 **camelCase**（`titleCn`/`coverUrl`），与旧库迁移过来的数据保持一致；
  记 snake_case 会让同一列出现两种写法。
- `updated_at` 由 SQLite 的 `strftime` 写，不引日期库。
- 校验放在 `GamePatch::validate()`：标题非空、发售日 `YYYY-MM-DD`（空串允许表示清空）、评分 0~10。
  命令层是公开入口，不能假设调用方一定是自己的界面。

---

## 9. 本期明确没做的事

- 没有数据库写入、没有真实扫描、没有网络请求
- 没有路由、没有状态管理库、没有暗色模式
- 业务按钮（启动/补丁/扫描/设置）点击只弹 Toast
- 封面色块是假的（但比例、遮罩、hover 与真实封面一致，替换时不动布局）

---

## 10. 下一步（B 阶段）

1. `src-tauri` 引入 `rusqlite`，建 `games / playtime / scan_roots / game_sources` 表（字段见第 2.2 节）
2. 写一次性迁移：读旧库 → 写新库，`path_hash` 原样搬（保住封面缓存可用），跳过 4 条测试夹具
3. `list_games` 命令 + 前端替换 `SEED_GAMES`
4. 保留 `StateSwitcher` 到 B 阶段结束，联调四态用得上

---

## 11. 目录扫描器的判定规则（C 阶段）

规则整体移植自旧版 `src/lib/scan.ts`，但改了一处**必须改**的地方（见下）。三条判定缺一不可：

| 层 | 作用 | 少了的后果 |
|---|---|---|
| 扩展名加权 | `.xp3`(6) / `.exe`(4) / `.arc`(2) / `.dat`(1) 累加 | 无法区分"有数据"和"只有程序" |
| **另有实锤** | 有数据文件 / 数据 ≥5MB / 子目录有 ≥5MB / 命中引擎特征 | `foo.exe + helper.dll + 说明.txt` 会被收成游戏 |
| **容器兜底** | 主程序在子目录时把父目录认成一部游戏 | `Game/bin/game.exe` + `Game/data/data.xp3` 会被拆成 `bin`、`data` 两条假条目 |

### 为什么改"容器"这一条（旧版没有的判据）

旧版只要"子树里只有一个可启动程序 + 有数据文件"就认定父目录是容器。
真机上这会把**会社目录**收成游戏：`E:/galgame/nekonekosoft/作品/game.exe` 里只有一个 exe，
于是 `nekonekosoft` 整条变成一部叫会社名的假游戏 —— 第一次扫描时这类假条目有 **31 条**
（`ALcot`、`DEVGRU-P`、`Others`、`nekonekosoft` …）。

补的判据：**看程序和数据的相对位置**
- 数据所在目录是程序所在目录的**同层或子层**（顶层路径段相同）→ 那个子目录自己是完整游戏 → 不是容器，继续下钻
- 两者在不同顶层段（`bin/` 与 `data/`）→ 才是"被拆开的游戏" → 父目录是容器

修正效果（真机实测，新增条目数）：**31 → 20**（加"位置"判据）**→ 18**（再加"临时/python 目录硬跳过"）。
剩下 18 条里约 5~6 条仍是工具目录（`GARbro` / `FreeMoteToolkit` / `lz4` / `Zcode` …），
它们自身带 `.xp3` 或数据文件，"像游戏"这件事规则层面无从分辨，留给用户级忽略清单。

### 其它必须守住的点

- **`path_hash` 必须按 UTF-16 码元迭代**（JS `charCodeAt` 语义），不是按码点。
  中日文在 BMP 内两者一致，遇到 emoji / 扩展区汉字会分叉。算错 = 已迁移的 211 部全部重复入库。
- **`upsert_scanned` 冲突时只更新扫描事实**（`file_count` / `dir_size` / `match_score` / `folder_path`），
  `title_cn` / `tags` / `rating` / `cover_url` 一律不动；`launch_exe` / `engine` 受 `locked_fields` 保护；
  `engine` 只在原值为 `unknown` 时被覆盖（扫描器更保守，不能把识别结果扫回去）。
- **进度事件必须节流**（100ms）。一本 200 部的库有上万次目录访问，不节流会把 webview 的消息队列打满，
  进度条反而卡住。
- **`State<Db>` 不能跨 `.await` 持有**（`MutexGuard` 不是 Send）。命令里用
  `spawn_blocking` + `app.state::<Db>()` 在阻塞线程内现取。

---

## 12. 扫描器前端接线（C 阶段收尾，2026-10-17）

### 12.1 选目录为什么走 Rust 命令，而不是前端 `@tauri-apps/plugin-dialog`

Tauri 的 dialog 插件有两套 API：JS 的 `open({directory:true})`，和 Rust 的 `DialogExt`。
这里选了 Rust：

1. **权限面更小**。JS 版要在 `capabilities/default.json` 里放 `dialog:*`，等于把"任意打开系统对话框"的
   权限开给 webview；Rust 版只是我们自己的一个 `pick_folder` 命令，权限边界仍然是"命令白名单"。
2. **符合项目铁律**：`invoke` 只出现在 `src/lib/api.ts`，组件里不 import Tauri 插件。
   多引一个 `@tauri-apps/plugin-dialog` 包，前端就多一条"绕过 api.ts 直接调系统能力"的路。
3. 前端只拿到一个字符串路径，之后照旧交给 `scan_directory(root)`，不碰文件系统。

### 12.2 为什么用回调 + oneshot，不用 `blocking_pick_folder()`

`blocking_*` 系列要求**调用线程所在的消息循环正在跑**。在 `async` 命令里调用它，会把 tokio 的
工作线程占死、主线程等这个工作线程 —— Windows 上表现为对话框不出现、窗口白屏。

正确写法是让插件自己在主线程泵消息，结果通过 oneshot 送回异步命令：

```rust
let (tx, rx) = tokio::sync::oneshot::channel();
app.dialog().file().set_title(..).pick_folder(move |p| { let _ = tx.send(p); });
rx.await
```

`tokio` 只开 `sync` 特性（`tauri` 本来就依赖 tokio，不会多编译一份运行时）。

### 12.3 进度为什么"塞进状态行"，而不是浮层或横条

试过浮层胶囊（`absolute top-16`）：会压住状态行的快捷键提示，看着像挡了一层东西。
也试过在顶栏下面插一条 36px 的横条：扫描开始/结束时整个网格上下跳一次，封面位置全变。

最终做法：书架顶部那行**本来就有**的右侧提示文案，扫描期间整段换成
`⟳ 正在扫描 1,234 个文件夹 · 发现 12 部 · <当前目录>`，扫完换回来。
同一行、同一位置、零位移，且这一行本来就不随滚动区移动。

配套：顶栏按钮在扫描期间禁用 + 文案变「扫描中…」+ 转圈。扫描没有"总目录数"，
所以**不做百分比进度条** —— 假的百分比比没有更糟。

### 12.4 扫完为什么整表 `reload()`，不像保存那样只替换一条

保存是"改一行"，所以用返回值替换本地那一条（见第 8 节）。
扫描是"批量新增 + 批量刷新扫描事实"，返回值里没有行数据，要精确合并就得再查一遍全表 ——
不如直接 `list_games()`。代价是会清空选中与光标，但扫描本来就是"我换了整个书架"的操作，
重拉后回到干净的浏览状态反而合理。

### 12.5 扫描失败不改 `gridState`

`gridState` 是"书架有没有内容"的状态。扫描失败时库里原有的 211 部还在，界面不该变成错误页。
所以失败只弹 Toast，列表保持原样；`error` 只留给"启动时读库失败"。

---

## 13. 忽略 vs 删除（两个动作，别合并）

扫描器分不出"工具目录"和"游戏"，这个坑最后落在用户身上：`GARbro` / `FreeMoteToolkit` 这类目录
本身带 `.xp3` 或数据文件，规则层面无从分辨（见第 11 节）。所以界面必须给两个不同的动作：

| | 忽略 | 删除 |
|---|---|---|
| 数据库 | 行保留，`ignored = 1` | 行删除，连带 playtime / 会话 / 来源 |
| 书架 | 不显示 | 不显示 |
| 重扫同一目录 | **不会**回来（`upsert_scanned` 从不碰 `ignored`） | **会**回来，作为新游戏 |
| 可否撤销 | 可以（设置 → 已忽略 → 恢复） | 不能，只能重扫 |
| 二次确认 | 不需要 | 必须 |

为什么忽略要保留行，而不是往 `scan_roots.ignore_paths` 里塞路径：
1. 路径式忽略解决的是"别再扫这个地方"，但用户真实意图是"这不是游戏"；
2. 保留行才能让设置面板列出"你忽略过什么"（带标题、能看出是什么东西）；
3. 重扫时 `upsert_scanned` 的冲突分支只更新扫描事实，`ignored` 天然保持 —— 零额外代码。

删除为什么必须把话说白："只删记录不动文件"和"重扫会回来"这两句不写在确认框里，
用户一定会以为自己在删游戏本体。

实现位置：`repo::set_ignored` / `repo::delete_games`（各带事务，在命令层包），
测试见 `repo::maintenance_tests`（4 个：忽略隐藏但保留行、重扫不解忽略、删除连带清理、删不存在的 id 计 0）。



---

## 12. 设置页的设计原则（2026-10-19）

设置页是**最容易长出假控件**的地方。这里定两条规矩：

1. **不放没有后端的开关**。写一个 `Switch` 但点了什么都不发生，比没有这个开关更糟 ——
   用户会以为功能坏了。所以刮削源、代理、凭据这些**等 D 阶段有了真实后端再往里加**，
   现在只在底部用一句话说明"后续阶段加到这里"。
2. **设置项必须能被验证**。「数据与关于」展示的每一项（数据库大小、schema 版本、
   封面缓存张数）都是打开面板时现算的，不是写死的文案。

### 扫描根目录为什么放在设置里

`scan_roots` 表在 V1 就建好了，但一直空着，导致每次扫描都要从磁盘里手翻目录
（`E:/galgame` 实测扫一次 90~105 秒，找目录这件事本身就很烦）。

做法：
- **扫描成功后自动登记**该目录（`repo::add_scan_root`），用户什么都不用做；
- 顶栏「扫描」在有登记目录时变成下拉：列出目录 + 上次扫描日期，最后一项是「选择其他目录…」；
- 设置页可以手动添加/移除/立即扫描某个目录。

路径会规范化（去尾部 `\`），否则 `E:/galgame` 与 `E:/galgame/` 会变成两条记录。
但**盘符根 `E:/` 必须保留分隔符** —— 去掉就变成盘符相对路径 `E:`，语义完全不同。

### 打开文件夹走 opener 插件的 Rust API

`tauri_plugin_opener::open_path` / `reveal_item_in_dir` 在 Rust 侧直接调用，
**不经过前端 ACL**，所以 `capabilities` 里不需要加 `opener:*` 权限。
前端只调我们自己的 `open_folder` / `reveal_path` 命令 —— 保持"invoke 只出现在 api.ts"的约定。

### 设置改成整页，而不是弹窗（2026-10-19）

原来的设置是个 600px 弹窗，一屏塞三块内容、只能上下滚。改成**整页 + 左侧分栏**：

- 顶栏 → 设置 → 整个画面切换成设置页；「返回书架」或 `Esc` 回去，书架状态（滚动位置、选中、筛选）原样保留。
- 左侧导航按「用户要办的事」分组（游戏库 / 数据 / 关于），**只放有真实后端的四项**。
- 参考过旧项目 MoeShelf 的设置页，但**只借用了"左侧分组 + 右侧内容"这个骨架**，没有照搬它的 12 个分区：
  它的「界面显示 / 大屏模式 / NSFW 模糊 / 代理 / AI 识别 / 局域网 / 手机串流」里有大半是空壳，
  点进去什么都改不了，导航越长越像半成品。等对应阶段有了后端再往导航里加，一次加一项。
- 内容区左对齐限宽 760，**不居中** —— 居中的表单在 1440 宽窗口里会像浮在半空。

---

## 13. 全屏详情页与角色数据（2026-10-19）

### 为什么单击/双击分两套

- **单击 = 选中**：右侧 320px 面板跟着换，适合快速扫一批游戏，不动视野。
- **双击 / Enter = 全屏详情**：角色网格一屏要放 12~20 张头像，320px 面板塞不下；
  抽屉式浮层也会被高度卡住，所以用整页（和设置页同一套做法：换掉整个画面，Esc 返回，书架状态原样保留）。
- `Enter` 与双击等价 —— 键盘用户不该被排除在这条路径之外。

### 角色数据是真的，不是占位

旧库 `scrape_cache.data_json` 里的 `characters` 数组：**196 部 / 2208 个角色，839 个带 cv**
（VNDB 抓角色与头像，Bangumi / cngal 补声优，`cvSource` 记录来源）。

新建 `characters` 表（schema V3）而不是塞进 `games` 的 JSON 列，理由：
1. 详情页要按角色逐条渲染，JSON 列每次都得 parse 一遍；
2. 以后要做「按声优筛选」得建索引，JSON 列建不了；
3. 整批替换比改 JSON 简单 —— 角色没有稳定主键（VNDB 的 `cid` 只有 7 条有），
   所以迁移时用「先删后插」，重跑不会累积重复角色。

### 两个数据现实，界面必须如实反映

1. **角色头像是远程 URL + 本地缓存**：2204 个角色有 image，但旧缓存只命中 1140 个。
   命中不了就渲染「名字首字 + 按名字生成的柔和底色」占位，**不显示裂图**。
2. **声优只覆盖 839/2208**：没收录的显示「声优未收录」（灰色小字），不隐藏这一行 ——
   隐藏会让人以为这位角色没有声优，其实是数据没抓到。

### 长简介必须折叠

真实数据里简介常有 700~1500 字，全展开会把角色区推到两三屏之外。
详情页折 8 行 + 「展开全部」；右侧面板折 4 行。两个阈值不同是有意的：
面板高度只有 600px 量级，详情页有一整屏。
