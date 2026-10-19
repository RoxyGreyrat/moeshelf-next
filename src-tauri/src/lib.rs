mod commands;
mod db;
mod models;
mod patch;
mod scrape;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 封面协议：cover://<8位hex> → 读旧库缓存里的 .bin，显式带上 Content-Type。
        // 前端在 Windows 上访问的是 http://cover.localhost/<hash>（Tauri 的协议映射规则）。
        .register_asynchronous_uri_scheme_protocol("cover", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let key = request
                .uri()
                .path()
                .trim_start_matches('/')
                .split('?')
                .next()
                .unwrap_or("")
                .to_string();

            // 异步应答：文件 IO 不能阻塞 webview 线程
            std::thread::spawn(move || {
                // 用户自己换过的封面在 app 数据目录里，优先读；没有再回落到旧库缓存
                match db::cover_cache::read_any(&app, &key) {
                    Some((bytes, ct)) => {
                        let response = tauri::http::Response::builder()
                            .status(200)
                            .header("Content-Type", ct)
                            .header("Access-Control-Allow-Origin", "*")
                            // 封面按 URL 内容寻址，内容不会变，可以长期缓存
                            .header("Cache-Control", "public, max-age=31536000, immutable")
                            .body(bytes)
                            .unwrap_or_default();
                        responder.respond(response);
                    }
                    None => {
                        let response = tauri::http::Response::builder()
                            .status(404)
                            .header("Access-Control-Allow-Origin", "*")
                            .body(Vec::new())
                            .unwrap_or_default();
                        responder.respond(response);
                    }
                }
            });
        })
        .setup(|app| {
            // 数据库在窗口出现前就绪：建库 + 建表 + 写 user_version
            let database = db::init(app.handle())?;

            // 上次保存的代理在第一次请求之前装进内存。
            // 读失败不影响启动：代理为空就是"自动"，业务照常。
            if let Ok(conn) = database.0.lock() {
                if let Ok(Some(proxy)) = db::repo::get_setting(&conn, scrape::PROXY_KEY) {
                    scrape::set_proxy(&proxy);
                }
            }

            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_games,
            commands::get_game_detail,
            commands::list_path,
            commands::set_cover_from_file,
            commands::set_cover_from_url,
            commands::reset_cover,
            commands::source_search,
            commands::source_apply,
            commands::get_scrape_settings,
            commands::set_scrape_settings,
            commands::auto_match_games,
            commands::cover_candidates,
            commands::get_network_settings,
            commands::set_network_settings,
            commands::test_network,
            commands::patch_list,
            commands::patch_preview,
            commands::patch_apply,
            commands::patch_revert,
            commands::patch_delete,
            commands::patch_scan,
            commands::patch_installer_begin,
            commands::patch_installer_finish,
            commands::update_game,
            commands::list_ignored_games,
            commands::set_games_ignored,
            commands::delete_games,
            commands::scan_directory,
            commands::list_scan_roots,
            commands::add_scan_root,
            commands::remove_scan_root,
            commands::open_folder,
            commands::reveal_path,
            commands::app_info,
            commands::migrate_legacy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
