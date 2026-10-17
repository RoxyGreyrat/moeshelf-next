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
