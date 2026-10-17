import { invoke } from "@tauri-apps/api/core";
import type { Game } from "@/types/game";

/**
 * 前端与 Rust 命令层的唯一出口。
 * 组件里不要直接写 invoke("xxx")，统一走这里，方便以后加日志/超时/错误兜底。
 */

/** 读取全部游戏。字段名与 Rust 侧 `#[serde(rename_all = "camelCase")]` 对应 */
export function listGames(): Promise<Game[]> {
  return invoke<Game[]>("list_games");
}

/** 插入 4 条演示数据，返回插入后的总行数（重复调用不会重复插入） */
export function addDummyGame(): Promise<number> {
  return invoke<number>("add_dummy_game");
}
