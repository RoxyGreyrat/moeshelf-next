mod commands;
mod db;
mod models;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 封面协议：cover://<8位hex> → 读旧库缓存里的 .bin，显式带上 Content-Type。
        // 前端在 Windows 上访问的是 http://cover.localhost/<hash>（Tauri 的协议映射规则）。
        .register_asynchronous_uri_scheme_protocol("cover", |_ctx, request, responder| {
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
                let base = db::cover_cache::dir();

                match db::cover_cache::read(&base, &key) {
                    Some(bytes) => {
                        let ct = db::cover_cache::content_type(&base, &key);
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
            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_games,
            commands::update_game,
            commands::add_dummy_game,
            commands::migrate_legacy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
