//! 建表语句与 schema 版本。
//!
//! 设计依据：`DECISIONS.md` 第 2.2 节（沿用 / 提升为列 / 新增 / 丢弃）。
//! 改动表结构时**必须**新建一版 `V2_DDL` 并把 `SCHEMA_VERSION` 加 1，
//! 再在 `db::init` 里按 `PRAGMA user_version` 选择要执行的批次 —— 不要直接改 V1。

pub const SCHEMA_VERSION: i64 = 4;

pub const V1_DDL: &str = r#"
-- ---------------------------------------------------------------- games
CREATE TABLE IF NOT EXISTS games (
    -- 主键沿用旧库的 8 位 hex 路径哈希：封面缓存文件名、playtime 关联都靠它
    id              TEXT PRIMARY KEY,
    path_hash       TEXT NOT NULL UNIQUE,

    -- 元数据：直接落真列，不做旧库那套"运行时镜像列"
    title           TEXT NOT NULL DEFAULT '',   -- 罗马字 / 英文
    title_cn        TEXT NOT NULL DEFAULT '',   -- 中文（officialCnTitle 优先）
    original_title  TEXT NOT NULL DEFAULT '',   -- 日文原名
    developer       TEXT NOT NULL DEFAULT '',
    released        TEXT NOT NULL DEFAULT '',   -- YYYY-MM-DD
    engine          TEXT NOT NULL DEFAULT 'unknown',  -- 独立列：左筛选栏要用
    platform        TEXT NOT NULL DEFAULT 'windows',
    tags            TEXT NOT NULL DEFAULT '[]', -- JSON 字符串数组
    rating          REAL NOT NULL DEFAULT 0,
    rating_source   TEXT NOT NULL DEFAULT '',   -- vndb | bangumi
    description     TEXT NOT NULL DEFAULT '',
    cn_description  TEXT NOT NULL DEFAULT '',
    cover_url       TEXT,

    -- 扫描产物
    launch_exe      TEXT NOT NULL DEFAULT '',
    folder_path     TEXT NOT NULL,
    folder_name     TEXT NOT NULL DEFAULT '',
    file_count      INTEGER NOT NULL DEFAULT 0,
    match_score     REAL NOT NULL DEFAULT 0,
    root_path       TEXT NOT NULL DEFAULT '',

    -- 状态与时间
    completed       INTEGER NOT NULL DEFAULT 0,
    completed_at    TEXT,
    saved_at        TEXT NOT NULL DEFAULT '',
    added_at        TEXT NOT NULL DEFAULT '',
    updated_at      TEXT NOT NULL DEFAULT '',
    first_seen_at   TEXT NOT NULL DEFAULT '',   -- 只写一次，重扫不覆盖

    -- 覆盖与来源（替代旧库 settings.json 里的 7 张 *Map）
    locked_fields   TEXT NOT NULL DEFAULT '[]', -- ["titleCn","coverUrl"]
    field_sources   TEXT NOT NULL DEFAULT '{}', -- {"title":"vndb","tags":"bangumi"}
    manual_match    TEXT,                       -- {"source":"vndb","id":"v25328"}
    ignored         INTEGER NOT NULL DEFAULT 0,

    vndb_id         TEXT,
    bangumi_id      INTEGER
);

CREATE INDEX IF NOT EXISTS idx_games_engine       ON games(engine);
CREATE INDEX IF NOT EXISTS idx_games_platform     ON games(platform);
CREATE INDEX IF NOT EXISTS idx_games_developer    ON games(developer);
CREATE INDEX IF NOT EXISTS idx_games_folder_path  ON games(folder_path);
CREATE INDEX IF NOT EXISTS idx_games_first_seen   ON games(first_seen_at);
CREATE INDEX IF NOT EXISTS idx_games_completed    ON games(completed);

-- ---------------------------------------------------------------- playtime
CREATE TABLE IF NOT EXISTS playtime (
    game_hash   TEXT PRIMARY KEY,
    minutes     REAL NOT NULL DEFAULT 0,
    sessions    INTEGER NOT NULL DEFAULT 0,
    last_played TEXT
);

-- 旧库只有一个累计值，看不到"哪天玩的"。这里按会话记账，playtime 仍然保留为汇总。
CREATE TABLE IF NOT EXISTS playtime_sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    game_hash   TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    ended_at    TEXT,
    seconds     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_sessions_hash ON playtime_sessions(game_hash);

-- ---------------------------------------------------------------- scan_roots
-- 替代旧库的单值 rootPath：支持多个根目录，各自开关与排除项
CREATE TABLE IF NOT EXISTS scan_roots (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    path         TEXT NOT NULL UNIQUE,
    enabled      INTEGER NOT NULL DEFAULT 1,
    ignore_paths TEXT NOT NULL DEFAULT '[]',
    last_scan_at TEXT,
    added_at     TEXT NOT NULL
);

-- ---------------------------------------------------------------- game_sources
-- 多源原始记录：某个源挂了或数据回滚，不丢其他源
CREATE TABLE IF NOT EXISTS game_sources (
    game_hash  TEXT NOT NULL,
    source     TEXT NOT NULL,              -- vndb | bangumi | ymgal | manual
    source_id  TEXT,
    payload    TEXT NOT NULL DEFAULT '{}',
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (game_hash, source)
);
"#;

/// V2：目录扫描器需要记录"这个游戏占多大"。
///
/// 旧库只有 `file_count`（而且是**直接子项数**，不是递归数），没有体积字段。
/// 扫描器要报告"目录大小"，所以补一列。`ALTER TABLE ADD COLUMN` 带默认值，
/// 老数据自动填 0，不需要回填。
pub const V2_DDL: &str = r#"
ALTER TABLE games ADD COLUMN dir_size INTEGER NOT NULL DEFAULT 0;
"#;

/// V3：角色与声优。
///
/// 数据来源是旧库刮削缓存里的 `characters` 数组（VNDB 抓角色、Bangumi/cngal 补声优）：
/// 196 部有角色、共 2208 个角色，其中 839 个带 cv。
/// 单独建表而不是塞进 games.tags 之类的 JSON 列，因为详情页要按角色逐个渲染，
/// 而且以后要做"按声优筛选"（一列 JSON 没法建索引）。
pub const V3_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS characters (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    game_hash   TEXT NOT NULL,
    name        TEXT NOT NULL,
    aliases     TEXT NOT NULL DEFAULT '[]',
    role        TEXT NOT NULL DEFAULT '',
    cv          TEXT NOT NULL DEFAULT '',
    cv_source   TEXT NOT NULL DEFAULT '',
    image_url   TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_characters_game ON characters(game_hash, sort_order);
"#;

/// V4：应用级设置（键值对）。
///
/// 只存"必须跨重启保留、又不属于某一条游戏"的东西，目前就一个 `network.proxy`。
/// 不沿用旧库的 settings.json：那是一个混着 exeMap / 各种 *Map 的大 JSON，
/// 每次改一个开关都要整份重写，冲突起来没法处理。
pub const V4_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);
"#;
