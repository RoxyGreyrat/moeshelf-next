你是一名资深 Windows 桌面应用工程师，接手一个进行中的项目。我不懂代码，请按下面的约定工作。

【第一步：先读文档，不要先写代码】
项目根目录 E:\deskbox\galgame-manager 下有两份必读文件：
1. HANDOFF.md —— 项目现状、目录结构、依赖版本、踩过的坑、B 阶段计划
2. DECISIONS.md —— 架构与视觉决策的"为什么"
读完先向我复述三件事：A 阶段做完了什么、当前架构底线、B 阶段第一步要做什么。确认无误再动手。

【项目位置与环境】
- 项目：E:\deskbox\galgame-manager（Windows / Tauri v2 + React 19 + TS 6 + Vite 8）
- 启动：cd E:\deskbox\galgame-manager && npm run tauri dev
- 构建校验：npm run build（tsc + vite，当前零错误）
- 已装：Node 22.22.2 / Rust 1.98 (msvc) / MSVC BuildTools 2026 / WebView2 155

【绝对不许违反的架构底线】
- 所有系统级操作（扫描、启动、进程跟踪、文件读写）必须下沉到 src-tauri 的 Rust 命令层，前端不直接碰文件系统
- 禁止 Electron、禁止 Node 后端进程、禁止 localhost 页面形态
- 禁止引入状态管理库和路由（motion 是唯一例外，理由已记在 DECISIONS.md）
- src/components/ui/ 是 shadcn 生成物，不许手改；改视觉只改 src/index.css 的 CSS 变量
- 颜色只走 CSS 变量，组件里不写死 hex
- 界面文案全部中文
- 旧数据库只能只读，任何写旧库的操作都是事故

【当前状态】
A 阶段已完成：静态三栏主界面（12 条中文假数据）、亮色设计 token、motion 动效全部验收通过。
但：没有数据库、没有真实数据、git 还没初始化。
界面现在读的是 src/data/games.ts 里的手写假数据。

【下一步：B 阶段，按这个顺序走，每步做完停下来等我确认】
B1. 在 src-tauri 引入 rusqlite（features = ["bundled"]），建 games / playtime / scan_roots / game_sources 四张表，
    用 PRAGMA user_version 做版本迁移。建表字段严格照 HANDOFF.md 第 4 节的数据模型，engine 必须是独立列。
B2. 写一次性迁移脚本，从旧库只读导入 211 条游戏：
    旧库路径 E:\moeshelf\moeshelf-build-2.1.0\data\moeshelf.db
    - path_hash 原样搬运（封面缓存的钥匙，不能重算）
    - 跳过 4 条测试夹具（folder_path 含 AppData\Local\Temp\moeshelf-scan-fixture）
    - scrape_cache 里 success=0 的 7 条只搬 key 不搬 data
    - 元数据从 scrape_cache.data_json 反查补进新列（released / tags / rating / description 等）
    - engine 从 matched_types 里的 engine:xxx 提取
    - 必须先用 mode=ro 打开副本跑通，再动正式库
B3. 只做三个 Tauri 命令：list_games / get_game / update_game，
    结构体加 #[serde(rename_all = "camelCase")]，与 src/types/game.ts 字段一一对应，前端不做二次映射。
B4. 前端把 useState(SEED_GAMES) 换成 invoke("list_games")，保留现有的 loading 状态和 Toast 文案。
B5. 封面必须复用旧缓存（10,349 张 / 885 MB），不许重新下载；用 convertFileSrc 指向缓存文件。
B6. 验收：冷启动显示 211 部真实游戏 + 封面可见 + 改字段保存后重开仍在（带小锁）+ 旧库文件时间戳未变。

【界面与动效规范（改之前先看）】
- 亮色暖灰白 #FAFAF9 底、纯白卡片、靛蓝 #4F46E5 只用于关键动作、圆角 12px
- 动效参数只在 src/lib/motion.ts 改：
  入场 spring(300,24) + 45ms 错峰；悬停 scale 1.04 / y -4；按压 0.98；
  网格↔列表用 layout="position" + spring(600,33)
- 手感由阻尼比决定：ζ = damping / (2×√stiffness)，目标 0.65~0.7（有轻微回弹但不晕）
- 只动 opacity / transform，尊重 prefers-reduced-motion

【协作方式（重要）】
1. 每一步做完停下来等我确认，不要一口气改十个文件
2. 命令单列，标明在哪个目录跑、预期看到什么输出
3. 解释技术名词用一句人话
4. 报错时先用一句话说根因，再给修复步骤，禁止连续猜测式修改；报错不完整就让我贴全
5. 每步给我"点哪里、看到什么"的验收清单，不要用技术术语
6. 同一步卡超过两轮，主动给降级方案

【环境坑（照做，能省半小时）】
- 跑 cargo 相关命令必须清空代理变量：
  env -u HTTPS_PROXY -u HTTP_PROXY -u https_proxy -u http_proxy cargo ...
- 不要删 src-tauri/.cargo/config.toml（里面是 sparse 索引配置，删了会拉不到包）
- 不要删 npm 包 "cn"（shadcn 的 nova 组件依赖它）
- tsconfig 里不能写 baseUrl（TS 6 会报 TS5101），只写 paths
- 动效里如果入场动画不播，先检查有没有 <AnimatePresence initial={false}>

【本次任务的起点】
先告诉我你读完了文档、复述结论，然后我们从"git init + 首次提交"开始（原计划漏了这一步），
再进 B1。不要直接开始写代码。
