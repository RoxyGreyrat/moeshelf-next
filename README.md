# MoeShelf Next

把散落在磁盘里的 galgame 整理成一个能看、能找、能直接启动的书架。

本地优先：游戏数据存在本地 SQLite 里，封面读本地缓存，不联网也能用；
联网只发生在你主动点「获取资料 / 补全资料」的时候。

## 功能

- **书架**：网格 / 列表两种视图，可按发售日、名称、评分、入库时间排序，正倒序切换。
- **筛选**：左侧面板按会社、引擎、平台过滤，支持搜索与分组折叠。
- **资料刮削**：从 VNDB 拉标题、封面、评分、发售日、标签；从月幕 / Bangumi 补中文简介、
  角色与声优（角色按名字对上才写，不会张冠李戴）。
- **启动器**：首次启动让你确认启动程序（目录里的 exe 列表 + 手动选择），之后直接启动；
  记录启动次数、游玩时长、上次游玩时间，运行中可结束进程。
- **目录扫描**：按目录结构与文件特征识别引擎（Kirikiri / Ren'Py / Unity / RPG Maker 等），
  猜主程序与体积；扫到一半可以**取消**，已经扫到的照样入库。扫描结果可忽略、可删除。
- **补丁管理**：预览会覆盖哪些文件 → 备份 → 应用 → 一键还原；支持 zip / 7z / rar / tar。
- **数据库备份**：设置里一键导出带时间戳的库快照（SQLite `VACUUM INTO`，不用关软件）。
- **界面状态记忆**：视图、排序、筛选、搜索词、选中项、侧栏收起状态，关掉重开都还在。
- **本地优先**：SQLite + 本地封面缓存，不需要账号，不上传任何东西。

## 技术栈

| 层 | 用的什么 |
|---|---|
| 前端 | React 19 · TypeScript · Vite · Tailwind CSS v4 · shadcn/ui · motion |
| 桌面 | Tauri v2（Rust） |
| 数据 | SQLite（rusqlite，bundled）+ 自定义 `cover://` 协议直读本地封面缓存 |
| 打包 | NSIS 安装包（Windows） |

## 开发

```bash
npm install
npm run tauri dev
```

要求：Node 20+、Rust stable、WebView2（Windows 10/11 自带）。

## 构建

```bash
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/nsis/`。

## 数据位置

- 开发期：`src-tauri/data/moeshelf.db`
- 安装后：`%APPDATA%\com.moeshelf.next\data\`
- **便携模式**：如果 exe 同级的 `data\moeshelf.db` 存在，就直接用那个目录 —— 软件放哪，数据就在哪

## 许可

MIT
