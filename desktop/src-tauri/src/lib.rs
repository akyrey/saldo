//! The desktop shell. Everything the user sees is the web app from `web/`;
//! this code only adds what a browser cannot do: the OS keychain (token.rs),
//! "Log in with Authentik" through the system browser (loopback.rs),
//! native notifications, opening links in the default browser, keeping
//! the window's size and position, and installing new versions (the updater
//! reads `latest.json` from the newest GitHub release and checks its
//! signature; `process` restarts the app afterwards).
//!
//! Tauri uses the operating system's own web view (WebKit on macOS, WebView2
//! on Windows) instead of bundling Chromium, like a PHP app relying on the
//! system's web server instead of shipping one: the app stays small.

mod loopback;
mod token;

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            token::load_token,
            token::save_token,
            token::delete_token,
            loopback::oidc_listen
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Saldo could not start: {error}");
        std::process::exit(1);
    }
}
