#[cfg(any(feature = "sdl_ui", all(feature = "lvgl_ui", target_os = "linux")))]
use std::collections::VecDeque;

use crate::ui::{
    catalog::{app_at, page},
    input::UiInputEvent,
    keyboard::{KEYBOARD_MAX_COLS, KEYBOARD_ROWS},
    model::{UiFrame, UiPage},
};

use super::lvgl_core::TOP_BAR_HEIGHT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointerSwipeAction {
    PrevPage,
    NextPage,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointerTapAction {
    OpenLauncherApp { row: usize, col: usize },
    KeyboardKey { row: usize, col: usize },
    KeyboardCancel,
    KeyboardSubmit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PointerUiSnapshot {
    page: UiPage,
    launcher_page: usize,
    selected_row: usize,
    selected_col: usize,
    width: i32,
    height: i32,
    keyboard_active: bool,
    keyboard_row_lengths: [usize; KEYBOARD_ROWS],
    keyboard_row_offsets: [u8; KEYBOARD_ROWS],
    keyboard_key_spans: [[u8; KEYBOARD_MAX_COLS]; KEYBOARD_ROWS],
}

#[derive(Debug, Default)]
pub(super) struct PointerGestureState {
    pub(super) pressed: bool,
    start: (i32, i32),
    current: (i32, i32),
}

#[derive(Debug, Default)]
pub(super) struct PointerInputAdapter {
    snapshot: Option<PointerUiSnapshot>,
    pub(super) gesture: PointerGestureState,
    pending_events: VecDeque<UiInputEvent>,
}

impl PointerUiSnapshot {
    pub(super) fn from_frame(frame: &UiFrame, width: u32, height: u32) -> Self {
        let mut keyboard_row_lengths = [0; KEYBOARD_ROWS];
        let mut keyboard_row_offsets = [0; KEYBOARD_ROWS];
        let mut keyboard_key_spans = [[0; KEYBOARD_MAX_COLS]; KEYBOARD_ROWS];
        if let Some(keyboard) = frame.keyboard.as_ref() {
            for (idx, row) in keyboard.layout_rows().iter().enumerate().take(4) {
                keyboard_row_lengths[idx] = row.keys.len();
                keyboard_row_offsets[idx] = row.offset_half_units;
                for (col, key) in row.keys.iter().enumerate().take(KEYBOARD_MAX_COLS) {
                    keyboard_key_spans[idx][col] = key.span_half_units;
                }
            }
        }
        Self {
            page: frame.page,
            launcher_page: frame.launcher_page,
            selected_row: frame.selected_row,
            selected_col: frame.selected_col,
            width: width as i32,
            height: height as i32,
            keyboard_active: frame.keyboard.is_some(),
            keyboard_row_lengths,
            keyboard_row_offsets,
            keyboard_key_spans,
        }
    }

    fn hit_test_keyboard(&self, x: i32, y: i32) -> Option<PointerTapAction> {
        if !self.keyboard_active {
            return None;
        }

        if x >= 14 && x < 122 && y >= 38 && y < 120 {
            return Some(PointerTapAction::KeyboardCancel);
        }
        if x >= self.width - 136 && x < self.width - 14 && y >= 38 && y < 120 {
            return Some(PointerTapAction::KeyboardSubmit);
        }

        let top_h = self.height / 3;
        let key_area_top = top_h + 10;
        let key_area_h = self.height - key_area_top - 14;
        let key_gap = 7;
        let key_h = (key_area_h - key_gap * 3) / 4;
        if y < key_area_top || y >= key_area_top + key_area_h {
            return None;
        }

        let row = ((y - key_area_top) / (key_h + key_gap)) as usize;
        if row >= KEYBOARD_ROWS {
            return None;
        }
        let row_y = key_area_top + row as i32 * (key_h + key_gap);
        if y >= row_y + key_h {
            return None;
        }

        let row_len = self.keyboard_row_lengths[row].min(KEYBOARD_MAX_COLS);
        let row_gap_count = row_len.saturating_sub(1) as i32;
        let total_half_units = (i32::from(self.keyboard_row_offsets[row])
            + (0..row_len)
                .map(|col| i32::from(self.keyboard_key_spans[row][col]))
                .sum::<i32>())
        .max(1);
        let half_unit_w = (self.width - 28 - key_gap * row_gap_count) / total_half_units;
        let mut cursor_x = 14 + i32::from(self.keyboard_row_offsets[row]) * half_unit_w;
        for col in 0..row_len {
            let span = i32::from(self.keyboard_key_spans[row][col]);
            let key_w = half_unit_w * span + key_gap * (span / 2 - 1).max(0);
            if x >= cursor_x && x < cursor_x + key_w {
                return Some(PointerTapAction::KeyboardKey { row, col });
            }
            cursor_x += key_w + key_gap;
        }

        None
    }

    fn hit_test_launcher_app(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        if !matches!(self.page, UiPage::Launcher) {
            return None;
        }

        let content_top = TOP_BAR_HEIGHT;
        if y < content_top {
            return None;
        }

        let p = page(self.launcher_page);
        let panel_h = (self.height - TOP_BAR_HEIGHT - 20).max(120);
        let panel_w = self.width - 40;
        let col_gap = 20;
        let row_gap = 25;
        let cols = p.cols.max(1) as i32;
        let cell_w = (panel_w - (cols - 1) * col_gap) / cols;
        let cell_h = 140;
        let is_home = self.launcher_page == 0;

        for row in 0..p.rows {
            for col in 0..p.cols {
                if app_at(self.launcher_page, row, col).is_none() {
                    continue;
                }
                let left = 20 + col as i32 * (cell_w + col_gap);
                let top = if is_home {
                    TOP_BAR_HEIGHT + panel_h - cell_h - 40
                } else {
                    TOP_BAR_HEIGHT + 20 + row as i32 * (cell_h + row_gap)
                };
                if x >= left && x < left + cell_w && y >= top && y < top + cell_h {
                    return Some((row, col));
                }
            }
        }

        None
    }

    fn tap_action(&self, x: i32, y: i32) -> Option<PointerTapAction> {
        if let Some(action) = self.hit_test_keyboard(x, y) {
            return Some(action);
        }
        if let Some((row, col)) = self.hit_test_launcher_app(x, y) {
            return Some(PointerTapAction::OpenLauncherApp { row, col });
        }
        None
    }

    fn swipe_action(&self, dx: i32) -> Option<PointerSwipeAction> {
        match self.page {
            UiPage::Launcher if dx <= -48 => Some(PointerSwipeAction::NextPage),
            UiPage::Launcher if dx >= 48 => Some(PointerSwipeAction::PrevPage),
            UiPage::App(_) if dx >= 48 => Some(PointerSwipeAction::Back),
            _ => None,
        }
    }
}

impl PointerInputAdapter {
    const TAP_SLOP: i32 = 32;
    const DRAG_PREVIEW_SLOP: i32 = 16;

    fn touch_debug_enabled() -> bool {
        std::env::var("LINTX_TOUCH_DEBUG")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
            .unwrap_or(false)
    }

    fn touch_debug_log(msg: &str) {
        if Self::touch_debug_enabled() {
            super::super::debug_log(msg);
        }
    }

    pub(super) fn update_snapshot(&mut self, frame: &UiFrame, width: u32, height: u32) {
        self.snapshot = Some(PointerUiSnapshot::from_frame(frame, width, height));
    }

    pub(super) fn pop_event(&mut self) -> Option<UiInputEvent> {
        self.pending_events.pop_front()
    }

    pub(super) fn drag_offset_x(&self) -> Option<i32> {
        if !self.gesture.pressed {
            return None;
        }

        let snapshot = self.snapshot?;
        let dx = self.gesture.current.0 - self.gesture.start.0;
        let dy = self.gesture.current.1 - self.gesture.start.1;

        if dx.abs() < Self::DRAG_PREVIEW_SLOP || dx.abs() <= dy.abs() {
            return None;
        }

        match snapshot.page {
            UiPage::Launcher => Some(dx),
            UiPage::App(_) if dx > 0 => Some(dx),
            UiPage::App(_) => None,
        }
    }

    pub(super) fn begin(&mut self, x: i32, y: i32) {
        Self::touch_debug_log(&format!("touch begin x={x} y={y}"));
        self.gesture.pressed = true;
        self.gesture.start = (x, y);
        self.gesture.current = (x, y);
    }

    pub(super) fn update(&mut self, x: i32, y: i32) {
        if self.gesture.pressed {
            Self::touch_debug_log(&format!("touch update x={x} y={y}"));
            self.gesture.current = (x, y);
        }
    }

    pub(super) fn end(&mut self, x: i32, y: i32) {
        if !self.gesture.pressed {
            return;
        }

        self.gesture.current = (x, y);
        self.gesture.pressed = false;

        let Some(snapshot) = self.snapshot else {
            return;
        };

        let dx = self.gesture.current.0 - self.gesture.start.0;
        let dy = self.gesture.current.1 - self.gesture.start.1;
        let abs_dx = dx.abs();
        let abs_dy = dy.abs();
        Self::touch_debug_log(&format!(
            "touch end x={} y={} dx={} dy={} abs_dx={} abs_dy={} page={:?} launcher_page={}",
            x, y, dx, dy, abs_dx, abs_dy, snapshot.page, snapshot.launcher_page
        ));

        if !snapshot.keyboard_active && abs_dx >= 48 && abs_dx > abs_dy {
            match snapshot.swipe_action(dx) {
                Some(PointerSwipeAction::PrevPage) => {
                    Self::touch_debug_log("touch gesture -> UiInputEvent::PagePrev");
                    self.pending_events.push_back(UiInputEvent::PagePrev)
                }
                Some(PointerSwipeAction::NextPage) => {
                    Self::touch_debug_log("touch gesture -> UiInputEvent::PageNext");
                    self.pending_events.push_back(UiInputEvent::PageNext)
                }
                Some(PointerSwipeAction::Back) => {
                    Self::touch_debug_log("touch gesture -> UiInputEvent::Back");
                    self.pending_events.push_back(UiInputEvent::Back)
                }
                None => {}
            }
            return;
        }

        if abs_dx <= Self::TAP_SLOP && abs_dy <= Self::TAP_SLOP {
            let tap_x = (self.gesture.start.0 + x) / 2;
            let tap_y = (self.gesture.start.1 + y) / 2;
            Self::touch_debug_log(&format!(
                "touch tap candidate x={} y={} start=({}, {}) end=({}, {})",
                tap_x, tap_y, self.gesture.start.0, self.gesture.start.1, x, y
            ));
            match snapshot.tap_action(tap_x, tap_y) {
                Some(PointerTapAction::OpenLauncherApp { row, col }) => {
                    Self::touch_debug_log(&format!(
                        "touch tap -> launcher app row={} col={}",
                        row, col
                    ));
                    for evt in self.align_selection(snapshot, row, col) {
                        Self::touch_debug_log(&format!("touch align -> {:?}", evt));
                        self.pending_events.push_back(evt);
                    }
                    Self::touch_debug_log("touch tap -> UiInputEvent::Open");
                    self.pending_events.push_back(UiInputEvent::Open);
                }
                Some(PointerTapAction::KeyboardKey { row, col }) => {
                    Self::touch_debug_log(&format!(
                        "touch tap -> keyboard key row={} col={}",
                        row, col
                    ));
                    self.pending_events
                        .push_back(UiInputEvent::KeyboardTap { row, col });
                }
                Some(PointerTapAction::KeyboardCancel) => {
                    Self::touch_debug_log("touch tap -> keyboard cancel");
                    self.pending_events.push_back(UiInputEvent::Back);
                }
                Some(PointerTapAction::KeyboardSubmit) => {
                    Self::touch_debug_log("touch tap -> keyboard submit");
                    self.pending_events.push_back(UiInputEvent::KeyboardSubmit);
                }
                None => {
                    Self::touch_debug_log("touch tap candidate -> no hit");
                }
            }
        }
    }

    fn align_selection(
        &self,
        snapshot: PointerUiSnapshot,
        row: usize,
        col: usize,
    ) -> std::vec::IntoIter<UiInputEvent> {
        let mut events = Vec::new();
        if !matches!(snapshot.page, UiPage::Launcher) {
            return events.into_iter();
        }

        for _ in row..snapshot.selected_row {
            events.push(UiInputEvent::Up);
        }
        for _ in snapshot.selected_row..row {
            events.push(UiInputEvent::Down);
        }
        for _ in col..snapshot.selected_col {
            events.push(UiInputEvent::Left);
        }
        for _ in snapshot.selected_col..col {
            events.push(UiInputEvent::Right);
        }

        events.into_iter()
    }
}
