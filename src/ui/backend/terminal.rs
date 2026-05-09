use std::io::Write;

use super::LvglBackend;
use crate::ui::{
    apps,
    catalog::{app_at, app_spec, page, PAGE_SPECS},
    input::UiInputEvent,
    model::{UiFrame, UiPage},
};

pub(super) struct TerminalBackend {
    backend_name: String,
}

impl TerminalBackend {
    pub(super) fn new(backend_name: String) -> Self {
        Self { backend_name }
    }
}

impl LvglBackend for TerminalBackend {
    fn init(&mut self) {
        println!("[ui] backend={} init", self.backend_name);
    }

    fn poll_event(&mut self) -> Option<UiInputEvent> {
        None
    }

    fn render(&mut self, frame: &UiFrame) {
        print!("\x1B[2J\x1B[1;1H");
        println!("LinTX Launcher [{}]", self.backend_name);
        println!(
            "STATUS | REM:{}% AIR:{}% SIG:{}% T:{}",
            frame.status.remote_battery_percent,
            frame.status.aircraft_battery_percent,
            frame.status.signal_strength_percent,
            frame.status.unix_time_secs
        );
        println!("-----------------------------------------------");
        match frame.page {
            UiPage::Launcher => {
                let p = page(frame.launcher_page);
                println!(
                    "                LinTX  [Page {}/{}]",
                    p.id + 1,
                    PAGE_SPECS.len()
                );
                for r in 0..p.rows {
                    for c in 0..p.cols {
                        if let Some(app) = app_at(frame.launcher_page, r, c) {
                            let mark = if r == frame.selected_row && c == frame.selected_col {
                                ">"
                            } else {
                                " "
                            };
                            print!("{} {:8}  ", mark, app_spec(app).title);
                        }
                    }
                    println!();
                }
                println!("Arrows move, Enter open, Esc back, [ ] switch page, Q quit");
            }
            UiPage::App(app) => {
                println!("APP: {}", app_spec(app).title);
                println!("{}", apps::render_terminal_detail(app, frame));
            }
        }
        if let Some(keyboard) = frame.keyboard.as_ref() {
            println!("-----------------------------------------------");
            println!(
                "KEYBOARD: {} [{}]{}",
                keyboard.label,
                keyboard.buffer,
                if keyboard.invalid { " INVALID" } else { "" }
            );
            println!(
                "Selected: row {} col {} key {}",
                keyboard.selected_row,
                keyboard.selected_col,
                keyboard.selected_key().label
            );
        }
        let _ = std::io::stdout().flush();
    }

    fn shutdown(&mut self) {
        println!("[ui] shutdown");
    }
}
