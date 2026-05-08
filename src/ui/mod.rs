pub mod app;
pub mod apps;
pub mod backend;
pub mod catalog;
pub mod feedback;
pub mod input;
pub mod model;
pub mod runner;

pub fn debug_enabled() -> bool {
    let ui_flag = std::env::var("LINTX_UI_DEBUG")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false);
    let common_flag = std::env::var("LINTX_DEBUG")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false);
    ui_flag || common_flag
}

pub fn debug_overlay_enabled() -> bool {
    let overlay_flag = std::env::var("LINTX_UI_DEBUG_OVERLAY")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false);
    overlay_flag || debug_enabled()
}

pub fn debug_log(msg: &str) {
    if debug_enabled() {
        eprintln!("[lintx-ui] {msg}");
    }
}
