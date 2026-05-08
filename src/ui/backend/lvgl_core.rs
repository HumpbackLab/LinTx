use crate::{
    messages::{UiFeedbackMotion, UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget},
    ui::{
        apps::models::visible_model_rows,
        catalog::{app_at, app_spec, page, PAGE_SPECS},
        feedback::UiFeedbackSnapshot,
        model::{AppId, UiFrame, UiPage},
    },
};

use super::{elrs_list_lines, signal_grade};

pub(super) const TOP_BAR_HEIGHT: i32 = 44;
pub(super) const LVGL_DRAW_BUF_PIXELS: usize = 800 * 480;

#[derive(Clone, Copy)]
pub(super) struct LvglUiObjects {
    pub(super) debug_panel: *mut lvgl_sys::lv_obj_t,
    pub(super) debug_label: *mut lvgl_sys::lv_obj_t,
    pub(super) status_label: *mut lvgl_sys::lv_obj_t,
    pub(super) clock_label: *mut lvgl_sys::lv_obj_t,
    pub(super) page_label: *mut lvgl_sys::lv_obj_t,
    pub(super) launcher_panel: *mut lvgl_sys::lv_obj_t,
    pub(super) launcher_panel_alt: *mut lvgl_sys::lv_obj_t,
    pub(super) app_panel: *mut lvgl_sys::lv_obj_t,
    pub(super) app_header_card: *mut lvgl_sys::lv_obj_t,
    pub(super) app_badge_label: *mut lvgl_sys::lv_obj_t,
    pub(super) app_title_label: *mut lvgl_sys::lv_obj_t,
    pub(super) app_subtitle_label: *mut lvgl_sys::lv_obj_t,
    pub(super) app_metric_cards: [*mut lvgl_sys::lv_obj_t; 2],
    pub(super) app_metric_titles: [*mut lvgl_sys::lv_obj_t; 2],
    pub(super) app_metric_values: [*mut lvgl_sys::lv_obj_t; 2],
    pub(super) app_metric_bars: [*mut lvgl_sys::lv_obj_t; 2],
    pub(super) app_list_title: *mut lvgl_sys::lv_obj_t,
    pub(super) app_list_row_cards: [*mut lvgl_sys::lv_obj_t; 4],
    pub(super) app_list_row_keys: [*mut lvgl_sys::lv_obj_t; 4],
    pub(super) app_list_row_vals: [*mut lvgl_sys::lv_obj_t; 4],
    pub(super) app_hint_label: *mut lvgl_sys::lv_obj_t,
    pub(super) branding_label: *mut lvgl_sys::lv_obj_t,
    pub(super) branding_label_alt: *mut lvgl_sys::lv_obj_t,
    pub(super) app_cards: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) app_cards_alt: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) app_icon_boxes: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) app_icon_boxes_alt: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) app_icon_labels: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) app_icon_labels_alt: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) app_title_labels: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) app_title_labels_alt: [*mut lvgl_sys::lv_obj_t; 8],
    pub(super) snapshot_layer: *mut lvgl_sys::lv_obj_t,
    pub(super) snapshot_img_primary: *mut lvgl_sys::lv_obj_t,
    pub(super) snapshot_img_secondary: *mut lvgl_sys::lv_obj_t,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppTemplateData {
    accent: (u8, u8, u8),
    badge: String,
    title: String,
    subtitle: String,
    metric_titles: [String; 2],
    metric_values: [String; 2],
    metric_progress: [u8; 2],
    list_title: String,
    list_lines: [String; 4],
    list_row_ids: [Option<String>; 4],
    hint: String,
    feedback: Option<UiFeedbackSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotScene {
    LauncherDrag {
        launcher_page: usize,
        alt_page: Option<usize>,
    },
    LauncherTransition {
        from_page: usize,
        to_page: usize,
    },
}

impl SnapshotScene {
    /// Check if two scenes are compatible for reusing the primary snapshot.
    /// This avoids rebuilding snapshots when only the secondary page changes during drag.
    fn is_compatible_with(&self, other: &SnapshotScene) -> bool {
        match (self, other) {
            (
                SnapshotScene::LauncherDrag {
                    launcher_page: a, ..
                },
                SnapshotScene::LauncherDrag {
                    launcher_page: b, ..
                },
            ) => a == b,
            (
                SnapshotScene::LauncherTransition {
                    from_page: a1,
                    to_page: a2,
                },
                SnapshotScene::LauncherTransition {
                    from_page: b1,
                    to_page: b2,
                },
            ) => a1 == b1 && a2 == b2,
            _ => self == other,
        }
    }

    /// Get the alt_page for LauncherDrag scenes
    fn alt_page(&self) -> Option<usize> {
        match self {
            SnapshotScene::LauncherDrag { alt_page, .. } => *alt_page,
            _ => None,
        }
    }
}

#[derive(Default)]
struct SnapshotAnimationState {
    scene: Option<SnapshotScene>,
    primary_dsc: Option<*mut lvgl_sys::lv_img_dsc_t>,
    secondary_dsc: Option<*mut lvgl_sys::lv_img_dsc_t>,
    active: bool,
}

pub(super) struct LvglUiCore {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) display: Option<lvgl::Display>,
    pub(super) ui: Option<LvglUiObjects>,
    pub(super) last_tick: std::time::Instant,
    drag_offset_x: Option<i32>,
    current_launcher_x: i32,
    target_launcher_x: i32,
    current_app_x: i32,
    target_app_x: i32,
    last_page: Option<UiPage>,
    last_launcher_page: usize,
    launcher_transition_from: Option<usize>,
    last_synced_frame: Option<UiFrame>,
    last_alt_launcher_page: Option<usize>,
    last_launcher_panel_pos: Option<(i32, i32)>,
    last_launcher_panel_alt_pos: Option<(i32, i32)>,
    last_app_panel_pos: Option<(i32, i32)>,
    debug_overlay_hidden: bool,
    snapshot: SnapshotAnimationState,
}

impl LvglUiCore {
    fn to_coord(v: i32) -> lvgl_sys::lv_coord_t {
        v.clamp(i16::MIN as i32, i16::MAX as i32) as lvgl_sys::lv_coord_t
    }

    fn color_make(r: u8, g: u8, b: u8) -> lvgl_sys::lv_color_t {
        unsafe { lvgl_sys::_LV_COLOR_MAKE(r, g, b) }
    }

    fn feedback_row_offset(feedback: &UiFeedbackSnapshot) -> i32 {
        match feedback.motion {
            UiFeedbackMotion::ShakeX => match (feedback.elapsed_ms / 40) % 6 {
                0 => 0,
                1 => -10,
                2 => 10,
                3 => -7,
                4 => 7,
                _ => 0,
            },
            UiFeedbackMotion::Pulse | UiFeedbackMotion::None => 0,
        }
    }

    fn feedback_accent(feedback: &UiFeedbackSnapshot) -> (u8, u8, u8) {
        match feedback.severity {
            UiFeedbackSeverity::Error => (224, 93, 93),
            UiFeedbackSeverity::Success => (88, 184, 120),
            UiFeedbackSeverity::Busy => (255, 196, 87),
        }
    }

    fn feedback_hint_color(feedback: &UiFeedbackSnapshot) -> (u8, u8, u8) {
        match feedback.severity {
            UiFeedbackSeverity::Error => (255, 168, 168),
            UiFeedbackSeverity::Success => (160, 232, 178),
            UiFeedbackSeverity::Busy => (255, 221, 133),
        }
    }

    pub(super) fn new(width: u32, height: u32) -> Self {
        let hidden_right = width as i32;
        Self {
            width,
            height,
            display: None,
            ui: None,
            last_tick: std::time::Instant::now(),
            drag_offset_x: None,
            current_launcher_x: 0,
            target_launcher_x: 0,
            current_app_x: hidden_right,
            target_app_x: hidden_right,
            last_page: None,
            last_launcher_page: 0,
            launcher_transition_from: None,
            last_synced_frame: None,
            last_alt_launcher_page: None,
            last_launcher_panel_pos: None,
            last_launcher_panel_alt_pos: None,
            last_app_panel_pos: None,
            debug_overlay_hidden: true,
            snapshot: SnapshotAnimationState::default(),
        }
    }

    fn set_label_text(label: *mut lvgl_sys::lv_obj_t, text: &str) {
        let sanitized = text.replace('\0', " ");
        if let Ok(c_text) = std::ffi::CString::new(sanitized) {
            unsafe {
                lvgl_sys::lv_label_set_text(label, c_text.as_ptr());
            }
        }
    }

    fn clamp_pct(v: i32) -> u8 {
        v.clamp(0, 100) as u8
    }

    fn set_obj_pos_if_changed(
        obj: *mut lvgl_sys::lv_obj_t,
        cache: &mut Option<(i32, i32)>,
        x: i32,
        y: i32,
    ) {
        if *cache == Some((x, y)) {
            return;
        }
        unsafe {
            lvgl_sys::lv_obj_set_pos(obj, Self::to_coord(x), Self::to_coord(y));
        }
        *cache = Some((x, y));
    }

    fn set_hidden_if_changed(obj: *mut lvgl_sys::lv_obj_t, hidden: bool, cache: &mut bool) {
        if *cache == hidden {
            return;
        }
        unsafe {
            if hidden {
                lvgl_sys::lv_obj_add_flag(obj, lvgl_sys::LV_OBJ_FLAG_HIDDEN);
            } else {
                lvgl_sys::lv_obj_clear_flag(obj, lvgl_sys::LV_OBJ_FLAG_HIDDEN);
            }
        }
        *cache = hidden;
    }

    fn set_obj_hidden(obj: *mut lvgl_sys::lv_obj_t, hidden: bool) {
        unsafe {
            if hidden {
                lvgl_sys::lv_obj_add_flag(obj, lvgl_sys::LV_OBJ_FLAG_HIDDEN);
            } else {
                lvgl_sys::lv_obj_clear_flag(obj, lvgl_sys::LV_OBJ_FLAG_HIDDEN);
            }
        }
    }

    pub(super) fn set_latest_display_default() {
        unsafe {
            let mut last = std::ptr::null_mut();
            let mut cur = lvgl_sys::lv_disp_get_next(std::ptr::null_mut());
            while !cur.is_null() {
                last = cur;
                cur = lvgl_sys::lv_disp_get_next(cur);
            }
            if !last.is_null() {
                lvgl_sys::lv_disp_set_default(last);
            }
        }
    }

    pub(super) fn start_frame(&mut self) {
        let now = std::time::Instant::now();
        let tick_ms = now
            .saturating_duration_since(self.last_tick)
            .as_millis()
            .clamp(1, 1000) as u32;
        self.last_tick = now;

        lvgl::tick_inc(std::time::Duration::from_millis(tick_ms as u64));
        lvgl::task_handler();
    }

    pub(super) fn set_drag_offset(&mut self, drag_offset_x: Option<i32>) {
        self.drag_offset_x = drag_offset_x;
    }

    fn hidden_right(&self) -> i32 {
        self.width as i32
    }

    fn hidden_left(&self) -> i32 {
        -self.hidden_right()
    }

    fn animate_axis(current: &mut i32, target: i32, step_factor: f32, snap_threshold: i32) {
        if *current == target {
            return;
        }

        let delta = target - *current;
        if delta.abs() <= snap_threshold {
            *current = target;
            return;
        }

        let min_step = (snap_threshold / 2).max(1);
        let step = ((delta.abs() as f32) * step_factor)
            .round()
            .max(min_step as f32) as i32;
        *current += step.min(delta.abs()) * delta.signum();
    }

    fn animate_launcher_axis(current: &mut i32, target: i32) {
        Self::animate_axis(current, target, 0.52, 28);
    }

    fn animate_app_axis(current: &mut i32, target: i32) {
        // Use a more aggressive curve so app enter/exit does not linger near the end points.
        Self::animate_axis(current, target, 0.62, 36);
    }

    fn snapshot_scene(&self, frame: &UiFrame, _prev_page: Option<UiPage>) -> Option<SnapshotScene> {
        match frame.page {
            UiPage::Launcher => {
                if let Some(drag_x) = self.drag_offset_x {
                    let alt_page = if drag_x < 0 && frame.launcher_page + 1 < PAGE_SPECS.len() {
                        Some(frame.launcher_page + 1)
                    } else if drag_x > 0 && frame.launcher_page > 0 {
                        Some(frame.launcher_page - 1)
                    } else {
                        None
                    };
                    return Some(SnapshotScene::LauncherDrag {
                        launcher_page: frame.launcher_page,
                        alt_page,
                    });
                }
                None
            }
            UiPage::App(_) => None,
        }
    }

    fn free_snapshot_dsc(slot: &mut Option<*mut lvgl_sys::lv_img_dsc_t>) {
        if let Some(ptr) = slot.take() {
            unsafe {
                lvgl_sys::lv_snapshot_free(ptr);
            }
        }
    }

    fn clear_snapshot_sources(&mut self, ui: &LvglUiObjects) {
        unsafe {
            lvgl_sys::lv_img_set_src(ui.snapshot_img_primary, std::ptr::null_mut());
            lvgl_sys::lv_img_set_src(ui.snapshot_img_secondary, std::ptr::null_mut());
        }
    }

    fn take_snapshot(obj: *mut lvgl_sys::lv_obj_t) -> Option<*mut lvgl_sys::lv_img_dsc_t> {
        let ptr = unsafe {
            lvgl_sys::lv_snapshot_take(obj, lvgl_sys::LV_IMG_CF_TRUE_COLOR as lvgl_sys::lv_img_cf_t)
        };
        (!ptr.is_null()).then_some(ptr)
    }

    fn app_template_data(&self, frame: &UiFrame, app: AppId) -> AppTemplateData {
        let spec = app_spec(app);
        match app {
            AppId::System => AppTemplateData {
                accent: spec.accent,
                badge: "SYSTEM".to_string(),
                title: "Power & Device Health".to_string(),
                subtitle: "Live status and quick parameter tuning".to_string(),
                metric_titles: ["Remote Battery".to_string(), "Aircraft Battery".to_string()],
                metric_values: [
                    format!("{}%", frame.status.remote_battery_percent),
                    format!("{}%", frame.status.aircraft_battery_percent),
                ],
                metric_progress: [
                    frame.status.remote_battery_percent,
                    frame.status.aircraft_battery_percent,
                ],
                list_title: "Quick Info".to_string(),
                list_lines: [
                    format!(
                        "Signal: {}% ({})",
                        frame.status.signal_strength_percent,
                        signal_grade(frame.status.signal_strength_percent)
                    ),
                    format!("Unix Time: {}", frame.status.unix_time_secs),
                    format!("Backlight: {}%", frame.config.backlight_percent),
                    format!("Sound: {}%", frame.config.sound_percent),
                ],
                list_row_ids: [None, None, None, None],
                hint: "UP/DOWN: Backlight   LEFT/RIGHT: Sound   Swipe right: Back".to_string(),
                feedback: frame.interaction_feedback.clone(),
            },
            AppId::Control => {
                let left_group =
                    &frame.input_frame.channels[..frame.input_frame.channels.len().min(4)];
                let right_group = if frame.input_frame.channels.len() > 4 {
                    &frame.input_frame.channels[4..frame.input_frame.channels.len().min(8)]
                } else {
                    &[]
                };
                let left_avg = if left_group.is_empty() {
                    0
                } else {
                    left_group
                        .iter()
                        .map(|value| i32::from(value.abs()))
                        .sum::<i32>()
                        / left_group.len() as i32
                };
                let right_avg = if right_group.is_empty() {
                    0
                } else {
                    right_group
                        .iter()
                        .map(|value| i32::from(value.abs()))
                        .sum::<i32>()
                        / right_group.len() as i32
                };
                AppTemplateData {
                    accent: spec.accent,
                    badge: "CONTROL".to_string(),
                    title: "Input Pipeline Monitor".to_string(),
                    subtitle: "Unified input source and mixer diagnostics".to_string(),
                    metric_titles: ["Source".to_string(), "Mixer".to_string()],
                    metric_values: [
                        format!(
                            "{} · {}",
                            frame.input_status.source.label(),
                            frame.input_status.health.label()
                        ),
                        format!(
                            "{}/{}/{}/{}",
                            frame.mixer_out.channels[0],
                            frame.mixer_out.channels[1],
                            frame.mixer_out.channels[2],
                            frame.mixer_out.channels[3]
                        ),
                    ],
                    metric_progress: [
                        Self::clamp_pct(left_avg * 100 / 2048),
                        Self::clamp_pct(right_avg * 100 / 2048),
                    ],
                    list_title: "Input Channels".to_string(),
                    list_lines: [
                        format!("Detail: {}", frame.input_status.detail),
                        format!(
                            "ELRS FB: {} S:{} B:{}",
                            if frame.elrs_feedback.connected {
                                "on"
                            } else {
                                "off"
                            },
                            frame
                                .elrs_feedback
                                .signal_strength_percent
                                .map(|v| format!("{}%", v))
                                .unwrap_or_else(|| "--".to_string()),
                            frame
                                .elrs_feedback
                                .aircraft_battery_percent
                                .map(|v| format!("{}%", v))
                                .unwrap_or_else(|| "--".to_string())
                        ),
                        format!(
                            "CH1-4: {}/{}/{}/{}",
                            frame.input_frame.channel_value(0),
                            frame.input_frame.channel_value(1),
                            frame.input_frame.channel_value(2),
                            frame.input_frame.channel_value(3),
                        ),
                        format!(
                            "Count:{} CH5-8:{}/{}/{}/{}",
                            frame.input_frame.channels.len(),
                            frame.input_frame.channel_value(4),
                            frame.input_frame.channel_value(5),
                            frame.input_frame.channel_value(6),
                            frame.input_frame.channel_value(7),
                        ),
                    ],
                    list_row_ids: [None, None, None, None],
                    hint: "Use for unified input -> mixer chain validation   Swipe right: Back"
                        .to_string(),
                    feedback: frame.interaction_feedback.clone(),
                }
            }
            AppId::Models => {
                let model_count = frame.model_entries.len().max(1);
                let focus = frame.model_focus_idx.min(model_count.saturating_sub(1));
                let active = frame.model_active_idx.min(model_count.saturating_sub(1));
                let focused_entry = frame.model_entries.get(focus);
                let active_entry = frame.model_entries.get(active);
                let metric_active = active_entry
                    .map(|entry| format!("{} · {}", entry.name, entry.protocol))
                    .unwrap_or_else(|| "No models".to_string());
                let metric_focus = focused_entry
                    .map(|entry| format!("{} · {}", entry.name, entry.protocol))
                    .unwrap_or_else(|| "No models".to_string());
                let visible_rows = visible_model_rows(frame, 4);
                let mut list_lines: Vec<String> = visible_rows
                    .iter()
                    .map(|row| {
                        format!(
                            "{} {}: {} · {}",
                            if row.is_focused { ">" } else { " " },
                            row.name,
                            if row.is_active { "ACTIVE" } else { "STORED" },
                            row.protocol
                        )
                    })
                    .collect();
                let mut list_row_ids = [None, None, None, None];
                for (idx, row) in visible_rows.iter().enumerate() {
                    list_row_ids[idx] = Some(row.id.clone());
                }
                if list_lines.is_empty() {
                    list_lines.push("No imported models found in ./models".to_string());
                }
                while list_lines.len() < 4 {
                    list_lines.push("".to_string());
                }
                AppTemplateData {
                    accent: spec.accent,
                    badge: "MODELS".to_string(),
                    title: "Model Profile Manager".to_string(),
                    subtitle: "Imported profiles from ./models".to_string(),
                    metric_titles: ["Active Profile".to_string(), "Focused Profile".to_string()],
                    metric_values: [metric_active, metric_focus],
                    metric_progress: [
                        Self::clamp_pct(((active + 1) * 100 / model_count) as i32),
                        Self::clamp_pct(((focus + 1) * 100 / model_count) as i32),
                    ],
                    list_title: "Profiles".to_string(),
                    list_lines: [
                        list_lines[0].clone(),
                        list_lines[1].clone(),
                        list_lines[2].clone(),
                        list_lines[3].clone(),
                    ],
                    list_row_ids,
                    hint:
                        "UP/DOWN: Focus Profile   ENTER: Apply   Files: ./models   Swipe right: Back"
                            .to_string(),
                    feedback: frame.interaction_feedback.clone(),
                }
            }
            AppId::Cloud => {
                let online = frame.cloud_connected;
                let sync_secs = if online {
                    frame
                        .status
                        .unix_time_secs
                        .saturating_sub(frame.cloud_last_sync_secs)
                } else {
                    0
                };
                AppTemplateData {
                    accent: spec.accent,
                    badge: "CLOUD".to_string(),
                    title: "Telemetry Link".to_string(),
                    subtitle: "Sync state and link quality".to_string(),
                    metric_titles: ["Connection".to_string(), "Link Quality".to_string()],
                    metric_values: [
                        if online {
                            "ONLINE".to_string()
                        } else {
                            "OFFLINE".to_string()
                        },
                        format!("{}%", frame.status.signal_strength_percent),
                    ],
                    metric_progress: [
                        if online { 100 } else { 0 },
                        frame.status.signal_strength_percent,
                    ],
                    list_title: "Sync Status".to_string(),
                    list_lines: [
                        format!("Last Sync: {}s ago", sync_secs),
                        format!("Remote Battery: {}%", frame.status.remote_battery_percent),
                        format!(
                            "Aircraft Battery: {}%",
                            frame.status.aircraft_battery_percent
                        ),
                        format!(
                            "Signal Class: {}",
                            signal_grade(frame.status.signal_strength_percent)
                        ),
                    ],
                    list_row_ids: [None, None, None, None],
                    hint: "ENTER: Connect/Disconnect   Swipe right: Back".to_string(),
                    feedback: frame.interaction_feedback.clone(),
                }
            }
            AppId::Scripts => {
                let list_lines = elrs_list_lines(frame);
                let mut list_row_ids = [None, None, None, None];
                if !frame.elrs.params.is_empty() {
                    let selected = frame
                        .elrs
                        .selected_idx
                        .min(frame.elrs.params.len().saturating_sub(1));
                    let start = selected
                        .saturating_sub(1)
                        .min(frame.elrs.params.len().saturating_sub(4));
                    for (slot, idx) in (start..(start + 4).min(frame.elrs.params.len())).enumerate()
                    {
                        list_row_ids[slot] = Some(frame.elrs.params[idx].id.clone());
                    }
                }
                AppTemplateData {
                    accent: spec.accent,
                    badge: "ELRS".to_string(),
                    title: "ExpressLRS Config".to_string(),
                    subtitle: format!(
                        "{} · {} · {}",
                        if frame.elrs.connected {
                            frame.elrs.module_name.as_str()
                        } else {
                            "Module not connected"
                        },
                        if frame.elrs.busy { "busy" } else { "ready" },
                        frame.elrs.path,
                    ),
                    metric_titles: ["UART / Link".to_string(), "RF / Power".to_string()],
                    metric_values: [
                        format!(
                            "{} · {}",
                            if frame.elrs.connected {
                                "UART ON"
                            } else {
                                "UART OFF"
                            },
                            if frame.elrs.link_active {
                                "LINKED"
                            } else {
                                "SEARCH"
                            }
                        ),
                        format!(
                            "{} · {}",
                            if frame.elrs.rf_output_enabled {
                                "RF ON"
                            } else {
                                "RF OFF"
                            },
                            frame.elrs.tx_power,
                        ),
                    ],
                    metric_progress: [
                        if frame.elrs.link_active {
                            100
                        } else if frame.elrs.connected {
                            40
                        } else {
                            0
                        },
                        if frame.elrs.rf_output_enabled { 100 } else { 0 },
                    ],
                    list_title: if frame.elrs.editor_active {
                        format!(
                            "Edit {} = {}",
                            frame.elrs.editor_label, frame.elrs.editor_buffer
                        )
                    } else {
                        format!("{} / {}", frame.elrs.device_name, frame.elrs.version)
                    },
                    list_lines,
                    list_row_ids,
                    hint: if frame.elrs.editor_active {
                        "UP/DOWN: Char   LEFT/RIGHT: Move   ENTER: Save   Swipe right: Cancel"
                            .to_string()
                    } else {
                        "UP/DOWN: Select   LEFT/RIGHT: Adjust   ENTER: Open/Apply   ]: Refresh   Swipe right: Back"
                            .to_string()
                    },
                    feedback: frame.interaction_feedback.clone(),
                }
            }
            AppId::UsbGamepad => AppTemplateData {
                accent: spec.accent,
                badge: "USB HID".to_string(),
                title: "USB Gamepad Output".to_string(),
                subtitle: "Expose mixer output as /dev/hidg0".to_string(),
                metric_titles: ["USB Gamepad".to_string(), "HID Device".to_string()],
                metric_values: [
                    if frame.usb_gamepad.running {
                        "ON".to_string()
                    } else {
                        "OFF".to_string()
                    },
                    if frame.usb_gamepad.hid_ready {
                        "READY".to_string()
                    } else {
                        "MISSING".to_string()
                    },
                ],
                metric_progress: [
                    if frame.usb_gamepad.running { 100 } else { 0 },
                    if frame.usb_gamepad.hid_ready { 100 } else { 0 },
                ],
                list_title: "Output Chain".to_string(),
                list_lines: [
                    format!(
                        "> Output: {}",
                        if frame.usb_gamepad.running {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    format!(
                        "HID: /dev/hidg0 ({})",
                        if frame.usb_gamepad.hid_ready {
                            "ready"
                        } else {
                            "missing"
                        }
                    ),
                    format!(
                        "Reports: {} ({})",
                        frame.usb_gamepad.report_count, frame.usb_gamepad.detail
                    ),
                    format!(
                        "Mixer: {}/{}/{}/{}",
                        frame.mixer_out.channels[0],
                        frame.mixer_out.channels[1],
                        frame.mixer_out.channels[2],
                        frame.mixer_out.channels[3],
                    ),
                ],
                list_row_ids: [Some("usb_gamepad_output".to_string()), None, None, None],
                hint: "ENTER/LEFT/RIGHT: Toggle ON/OFF   Swipe right: Back".to_string(),
                feedback: frame.interaction_feedback.clone(),
            },
            _ => {
                let spec = app_spec(app);
                let badge = spec.title.to_string();
                AppTemplateData {
                    accent: spec.accent,
                    title: format!("{} Workspace", badge),
                    badge,
                    subtitle: "Template placeholder".to_string(),
                    metric_titles: ["Metric A".to_string(), "Metric B".to_string()],
                    metric_values: ["--".to_string(), "--".to_string()],
                    metric_progress: [0, 0],
                    list_title: "Details".to_string(),
                    list_lines: [
                        "No data".to_string(),
                        "No data".to_string(),
                        "No data".to_string(),
                        "No data".to_string(),
                    ],
                    list_row_ids: [None, None, None, None],
                    hint: "Swipe right: Back".to_string(),
                    feedback: frame.interaction_feedback.clone(),
                }
            }
        }
    }

    fn prepare_snapshot_scene(
        &mut self,
        frame: &UiFrame,
        ui: &LvglUiObjects,
        scene: SnapshotScene,
    ) {
        match scene {
            SnapshotScene::LauncherDrag {
                launcher_page,
                alt_page,
            } => {
                self.update_launcher_panel(
                    launcher_page,
                    Some((frame.selected_row, frame.selected_col)),
                    ui.branding_label,
                    &ui.app_cards,
                    &ui.app_icon_boxes,
                    &ui.app_icon_labels,
                    &ui.app_title_labels,
                );
                if let Some(page_idx) = alt_page {
                    self.update_launcher_panel(
                        page_idx,
                        None,
                        ui.branding_label_alt,
                        &ui.app_cards_alt,
                        &ui.app_icon_boxes_alt,
                        &ui.app_icon_labels_alt,
                        &ui.app_title_labels_alt,
                    );
                }
            }
            SnapshotScene::LauncherTransition { from_page, to_page } => {
                self.update_launcher_panel(
                    to_page,
                    Some((frame.selected_row, frame.selected_col)),
                    ui.branding_label,
                    &ui.app_cards,
                    &ui.app_icon_boxes,
                    &ui.app_icon_labels,
                    &ui.app_title_labels,
                );
                self.update_launcher_panel(
                    from_page,
                    None,
                    ui.branding_label_alt,
                    &ui.app_cards_alt,
                    &ui.app_icon_boxes_alt,
                    &ui.app_icon_labels_alt,
                    &ui.app_title_labels_alt,
                );
            }
        }
    }

    fn rebuild_snapshot_scene(
        &mut self,
        frame: &UiFrame,
        ui: &LvglUiObjects,
        scene: SnapshotScene,
    ) -> bool {
        super::super::debug_log(&format!(
            "rebuild_snapshot_scene: starting scene={:?}",
            scene
        ));
        self.prepare_snapshot_scene(frame, ui, scene);
        Self::free_snapshot_dsc(&mut self.snapshot.primary_dsc);
        Self::free_snapshot_dsc(&mut self.snapshot.secondary_dsc);

        let (primary_obj, secondary_obj) = match scene {
            SnapshotScene::LauncherDrag { alt_page, .. } => {
                (ui.launcher_panel, alt_page.map(|_| ui.launcher_panel_alt))
            }
            SnapshotScene::LauncherTransition { .. } => {
                (ui.launcher_panel, Some(ui.launcher_panel_alt))
            }
        };

        self.snapshot.primary_dsc = Self::take_snapshot(primary_obj);
        self.snapshot.secondary_dsc = secondary_obj.and_then(Self::take_snapshot);
        if self.snapshot.primary_dsc.is_none() {
            super::super::debug_log("snapshot rebuild failed: primary_dsc is None");
            self.clear_snapshot_sources(ui);
            self.snapshot.scene = None;
            return false;
        }

        super::super::debug_log(&format!(
            "snapshot rebuild ok: scene={:?} primary={} secondary={}",
            scene,
            self.snapshot.primary_dsc.is_some(),
            self.snapshot.secondary_dsc.is_some()
        ));

        unsafe {
            lvgl_sys::lv_img_set_src(
                ui.snapshot_img_primary,
                self.snapshot
                    .primary_dsc
                    .map(|ptr| ptr.cast::<core::ffi::c_void>())
                    .unwrap_or(std::ptr::null_mut()),
            );
            lvgl_sys::lv_img_set_src(
                ui.snapshot_img_secondary,
                self.snapshot
                    .secondary_dsc
                    .map(|ptr| ptr.cast::<core::ffi::c_void>())
                    .unwrap_or(std::ptr::null_mut()),
            );
        }

        self.snapshot.scene = Some(scene);
        true
    }

    fn set_snapshot_overlay_active(&mut self, ui: &LvglUiObjects, active: bool) {
        super::super::debug_log(&format!("snapshot overlay active={}", active));
        if active {
            Self::set_obj_hidden(ui.snapshot_layer, false);
            Self::set_obj_hidden(ui.launcher_panel, true);
            Self::set_obj_hidden(ui.launcher_panel_alt, true);
            Self::set_obj_hidden(ui.app_panel, true);
        } else {
            Self::set_obj_hidden(ui.snapshot_layer, true);
            self.clear_snapshot_sources(ui);
            Self::set_obj_hidden(ui.launcher_panel, false);
            Self::set_obj_hidden(ui.launcher_panel_alt, false);
            Self::set_obj_hidden(ui.app_panel, false);
        }
        self.snapshot.active = active;
    }

    fn update_secondary_snapshot_for_drag(
        &mut self,
        _frame: &UiFrame,
        ui: &LvglUiObjects,
        alt_page: Option<usize>,
    ) {
        // Get the current alt_page from the stored scene
        let current_alt = self.snapshot.scene.and_then(|s| s.alt_page());

        if current_alt == alt_page {
            return; // No change needed
        }

        // Update the secondary snapshot for the new alt_page
        Self::free_snapshot_dsc(&mut self.snapshot.secondary_dsc);

        if let Some(page_idx) = alt_page {
            // Update the alt launcher panel content
            self.update_launcher_panel(
                page_idx,
                None,
                ui.branding_label_alt,
                &ui.app_cards_alt,
                &ui.app_icon_boxes_alt,
                &ui.app_icon_labels_alt,
                &ui.app_title_labels_alt,
            );
            self.snapshot.secondary_dsc = Self::take_snapshot(ui.launcher_panel_alt);
        }

        unsafe {
            lvgl_sys::lv_img_set_src(
                ui.snapshot_img_secondary,
                self.snapshot
                    .secondary_dsc
                    .map(|ptr| ptr.cast::<core::ffi::c_void>())
                    .unwrap_or(std::ptr::null_mut()),
            );
        }

        // Update the scene to reflect the new alt_page
        if let Some(SnapshotScene::LauncherDrag { launcher_page, .. }) = self.snapshot.scene {
            self.snapshot.scene = Some(SnapshotScene::LauncherDrag {
                launcher_page,
                alt_page,
            });
        }
    }

    fn ensure_snapshot_scene(
        &mut self,
        frame: &UiFrame,
        ui: &LvglUiObjects,
        scene: SnapshotScene,
    ) -> bool {
        let needs_rebuild = match &self.snapshot.scene {
            None => true,
            Some(existing) => !existing.is_compatible_with(&scene),
        };

        super::super::debug_log(&format!(
            "ensure_snapshot_scene: scene={:?} current={:?} needs_rebuild={}",
            scene, self.snapshot.scene, needs_rebuild
        ));

        if needs_rebuild {
            if !self.rebuild_snapshot_scene(frame, ui, scene) {
                return false;
            }
        } else if let SnapshotScene::LauncherDrag { alt_page, .. } = scene {
            // Scene is compatible but alt_page might have changed during drag
            self.update_secondary_snapshot_for_drag(frame, ui, alt_page);
        }

        if !self.snapshot.active {
            self.set_snapshot_overlay_active(ui, true);
        }
        true
    }

    fn teardown_snapshot_scene(&mut self, ui: &LvglUiObjects) {
        if self.snapshot.active {
            self.set_snapshot_overlay_active(ui, false);
        }
        Self::free_snapshot_dsc(&mut self.snapshot.primary_dsc);
        Self::free_snapshot_dsc(&mut self.snapshot.secondary_dsc);
        self.snapshot.scene = None;
    }

    fn layout_snapshot_launcher(&self, ui: &LvglUiObjects, secondary_x: Option<i32>) {
        match self.snapshot.scene {
            Some(SnapshotScene::LauncherDrag { .. }) => {
                unsafe {
                    lvgl_sys::lv_obj_set_pos(
                        ui.snapshot_img_primary,
                        Self::to_coord(0),
                        Self::to_coord(TOP_BAR_HEIGHT),
                    );
                }
                Self::set_obj_hidden(ui.snapshot_img_primary, self.snapshot.primary_dsc.is_none());

                if let Some(x) = secondary_x.filter(|_| self.snapshot.secondary_dsc.is_some()) {
                    unsafe {
                        lvgl_sys::lv_obj_set_pos(
                            ui.snapshot_img_secondary,
                            Self::to_coord(x),
                            Self::to_coord(TOP_BAR_HEIGHT),
                        );
                    }
                    Self::set_obj_hidden(ui.snapshot_img_secondary, false);
                } else {
                    Self::set_obj_hidden(ui.snapshot_img_secondary, true);
                }
            }
            Some(SnapshotScene::LauncherTransition { .. }) => {
                unsafe {
                    lvgl_sys::lv_obj_set_pos(
                        ui.snapshot_img_secondary,
                        Self::to_coord(0),
                        Self::to_coord(TOP_BAR_HEIGHT),
                    );
                }
                Self::set_obj_hidden(
                    ui.snapshot_img_secondary,
                    self.snapshot.secondary_dsc.is_none(),
                );

                if let Some(x) = secondary_x.filter(|_| self.snapshot.primary_dsc.is_some()) {
                    unsafe {
                        lvgl_sys::lv_obj_set_pos(
                            ui.snapshot_img_primary,
                            Self::to_coord(x),
                            Self::to_coord(TOP_BAR_HEIGHT),
                        );
                    }
                    Self::set_obj_hidden(ui.snapshot_img_primary, false);
                } else {
                    Self::set_obj_hidden(ui.snapshot_img_primary, true);
                }
            }
            None => {
                Self::set_obj_hidden(ui.snapshot_img_primary, true);
                Self::set_obj_hidden(ui.snapshot_img_secondary, true);
            }
        }
    }

    pub(super) fn build_ui(&mut self) {
        let width = self.width as i32;
        let height = self.height as i32;

        unsafe {
            let root = lvgl_sys::lv_disp_get_scr_act(std::ptr::null_mut());
            lvgl_sys::lv_obj_clean(root);
            lvgl_sys::lv_obj_set_style_bg_color(root, lvgl_sys::_LV_COLOR_MAKE(20, 20, 22), 0);
            lvgl_sys::lv_obj_clear_flag(root, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
            lvgl_sys::lv_obj_set_scrollbar_mode(
                root,
                lvgl_sys::LV_SCROLLBAR_MODE_OFF as lvgl_sys::lv_scrollbar_mode_t,
            );

            let status_label = lvgl_sys::lv_label_create(root);
            lvgl_sys::lv_obj_set_style_text_color(
                status_label,
                lvgl_sys::_LV_COLOR_MAKE(180, 180, 185),
                0,
            );
            lvgl_sys::lv_obj_set_pos(status_label, Self::to_coord(8), Self::to_coord(10));

            let debug_panel = lvgl_sys::lv_obj_create(root);
            lvgl_sys::lv_obj_set_pos(debug_panel, Self::to_coord(6), Self::to_coord(6));
            lvgl_sys::lv_obj_set_size(debug_panel, Self::to_coord(90), Self::to_coord(34));
            lvgl_sys::lv_obj_set_style_radius(debug_panel, 10, 0);
            lvgl_sys::lv_obj_set_style_bg_color(
                debug_panel,
                lvgl_sys::_LV_COLOR_MAKE(12, 14, 18),
                0,
            );
            lvgl_sys::lv_obj_set_style_bg_opa(debug_panel, 216, 0);
            lvgl_sys::lv_obj_set_style_border_width(debug_panel, 1, 0);
            lvgl_sys::lv_obj_set_style_border_color(
                debug_panel,
                lvgl_sys::_LV_COLOR_MAKE(66, 72, 84),
                0,
            );
            lvgl_sys::lv_obj_set_style_pad_top(debug_panel, 4, 0);
            lvgl_sys::lv_obj_set_style_pad_bottom(debug_panel, 4, 0);
            lvgl_sys::lv_obj_set_style_pad_left(debug_panel, 6, 0);
            lvgl_sys::lv_obj_set_style_pad_right(debug_panel, 6, 0);
            lvgl_sys::lv_obj_clear_flag(debug_panel, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
            lvgl_sys::lv_obj_add_flag(debug_panel, lvgl_sys::LV_OBJ_FLAG_HIDDEN);

            let debug_label = lvgl_sys::lv_label_create(debug_panel);
            lvgl_sys::lv_obj_set_style_text_color(
                debug_label,
                lvgl_sys::_LV_COLOR_MAKE(220, 226, 235),
                0,
            );
            lvgl_sys::lv_obj_set_style_text_font(
                debug_label,
                &lvgl_sys::lv_font_montserrat_14 as *const _ as *const lvgl_sys::lv_font_t,
                0,
            );
            lvgl_sys::lv_obj_align(
                debug_label,
                lvgl_sys::LV_ALIGN_CENTER as lvgl_sys::lv_align_t,
                0,
                0,
            );
            Self::set_label_text(debug_label, "FPS 0\nCPU --");

            let page_label = lvgl_sys::lv_label_create(root);
            lvgl_sys::lv_obj_set_style_text_color(
                page_label,
                lvgl_sys::_LV_COLOR_MAKE(180, 180, 185),
                0,
            );
            lvgl_sys::lv_obj_set_pos(
                page_label,
                Self::to_coord(width / 2 - 34),
                Self::to_coord(10),
            );

            let clock_label = lvgl_sys::lv_label_create(root);
            lvgl_sys::lv_obj_set_style_text_color(
                clock_label,
                lvgl_sys::_LV_COLOR_MAKE(180, 180, 185),
                0,
            );
            lvgl_sys::lv_obj_set_pos(clock_label, Self::to_coord(width - 90), Self::to_coord(10));

            let launcher_panel = lvgl_sys::lv_obj_create(root);
            lvgl_sys::lv_obj_set_pos(
                launcher_panel,
                Self::to_coord(0),
                Self::to_coord(TOP_BAR_HEIGHT),
            );
            lvgl_sys::lv_obj_set_size(
                launcher_panel,
                Self::to_coord(width),
                Self::to_coord(height - TOP_BAR_HEIGHT),
            );
            lvgl_sys::lv_obj_set_style_bg_color(
                launcher_panel,
                lvgl_sys::_LV_COLOR_MAKE(30, 30, 32),
                0,
            );
            lvgl_sys::lv_obj_set_style_bg_opa(launcher_panel, 255, 0);
            lvgl_sys::lv_obj_set_style_border_width(launcher_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_radius(launcher_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_top(launcher_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_bottom(launcher_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_left(launcher_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_right(launcher_panel, 0, 0);
            lvgl_sys::lv_obj_clear_flag(launcher_panel, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
            lvgl_sys::lv_obj_set_scrollbar_mode(
                launcher_panel,
                lvgl_sys::LV_SCROLLBAR_MODE_OFF as lvgl_sys::lv_scrollbar_mode_t,
            );

            let branding_label = lvgl_sys::lv_label_create(launcher_panel);
            Self::set_label_text(branding_label, "LinTX");
            lvgl_sys::lv_obj_set_style_text_color(
                branding_label,
                lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                0,
            );
            lvgl_sys::lv_obj_set_style_text_font(
                branding_label,
                &lvgl_sys::lv_font_montserrat_48 as *const _ as *const lvgl_sys::lv_font_t,
                0,
            );
            lvgl_sys::lv_obj_align(
                branding_label,
                lvgl_sys::LV_ALIGN_TOP_MID as lvgl_sys::lv_align_t,
                0,
                60,
            );

            let mut app_cards = [std::ptr::null_mut(); 8];
            let mut app_icon_boxes = [std::ptr::null_mut(); 8];
            let mut app_icon_labels = [std::ptr::null_mut(); 8];
            let mut app_title_labels = [std::ptr::null_mut(); 8];

            for i in 0..8 {
                let card = lvgl_sys::lv_obj_create(launcher_panel);
                lvgl_sys::lv_obj_set_style_bg_opa(card, 0, 0);
                lvgl_sys::lv_obj_set_style_border_width(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_top(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_bottom(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_left(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_right(card, 0, 0);
                lvgl_sys::lv_obj_clear_flag(card, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);

                let icon_box = lvgl_sys::lv_obj_create(card);
                lvgl_sys::lv_obj_set_style_radius(icon_box, 16, 0);
                lvgl_sys::lv_obj_set_style_border_width(icon_box, 0, 0);
                lvgl_sys::lv_obj_clear_flag(icon_box, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);

                let icon_label = lvgl_sys::lv_label_create(icon_box);
                lvgl_sys::lv_obj_set_style_text_color(
                    icon_label,
                    lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                    0,
                );
                lvgl_sys::lv_obj_align(
                    icon_label,
                    lvgl_sys::LV_ALIGN_CENTER as lvgl_sys::lv_align_t,
                    0,
                    0,
                );

                let title_label = lvgl_sys::lv_label_create(card);
                lvgl_sys::lv_obj_set_style_text_color(
                    title_label,
                    lvgl_sys::_LV_COLOR_MAKE(220, 220, 220),
                    0,
                );
                lvgl_sys::lv_obj_set_style_text_align(
                    title_label,
                    lvgl_sys::LV_TEXT_ALIGN_CENTER as lvgl_sys::lv_text_align_t,
                    0,
                );

                app_cards[i] = card;
                app_icon_boxes[i] = icon_box;
                app_icon_labels[i] = icon_label;
                app_title_labels[i] = title_label;
            }

            let launcher_panel_alt = lvgl_sys::lv_obj_create(root);
            lvgl_sys::lv_obj_set_pos(
                launcher_panel_alt,
                Self::to_coord(width),
                Self::to_coord(TOP_BAR_HEIGHT),
            );
            lvgl_sys::lv_obj_set_size(
                launcher_panel_alt,
                Self::to_coord(width),
                Self::to_coord(height - TOP_BAR_HEIGHT),
            );
            lvgl_sys::lv_obj_set_style_bg_color(
                launcher_panel_alt,
                lvgl_sys::_LV_COLOR_MAKE(30, 30, 32),
                0,
            );
            lvgl_sys::lv_obj_set_style_bg_opa(launcher_panel_alt, 255, 0);
            lvgl_sys::lv_obj_set_style_border_width(launcher_panel_alt, 0, 0);
            lvgl_sys::lv_obj_set_style_radius(launcher_panel_alt, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_top(launcher_panel_alt, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_bottom(launcher_panel_alt, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_left(launcher_panel_alt, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_right(launcher_panel_alt, 0, 0);
            lvgl_sys::lv_obj_clear_flag(launcher_panel_alt, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
            lvgl_sys::lv_obj_set_scrollbar_mode(
                launcher_panel_alt,
                lvgl_sys::LV_SCROLLBAR_MODE_OFF as lvgl_sys::lv_scrollbar_mode_t,
            );

            let branding_label_alt = lvgl_sys::lv_label_create(launcher_panel_alt);
            Self::set_label_text(branding_label_alt, "LinTX");
            lvgl_sys::lv_obj_set_style_text_color(
                branding_label_alt,
                lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                0,
            );
            lvgl_sys::lv_obj_set_style_text_font(
                branding_label_alt,
                &lvgl_sys::lv_font_montserrat_48 as *const _ as *const lvgl_sys::lv_font_t,
                0,
            );
            lvgl_sys::lv_obj_align(
                branding_label_alt,
                lvgl_sys::LV_ALIGN_TOP_MID as lvgl_sys::lv_align_t,
                0,
                60,
            );

            let mut app_cards_alt = [std::ptr::null_mut(); 8];
            let mut app_icon_boxes_alt = [std::ptr::null_mut(); 8];
            let mut app_icon_labels_alt = [std::ptr::null_mut(); 8];
            let mut app_title_labels_alt = [std::ptr::null_mut(); 8];

            for i in 0..8 {
                let card = lvgl_sys::lv_obj_create(launcher_panel_alt);
                lvgl_sys::lv_obj_set_style_bg_opa(card, 0, 0);
                lvgl_sys::lv_obj_set_style_border_width(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_top(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_bottom(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_left(card, 0, 0);
                lvgl_sys::lv_obj_set_style_pad_right(card, 0, 0);
                lvgl_sys::lv_obj_clear_flag(card, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);

                let icon_box = lvgl_sys::lv_obj_create(card);
                lvgl_sys::lv_obj_set_style_radius(icon_box, 16, 0);
                lvgl_sys::lv_obj_set_style_border_width(icon_box, 0, 0);
                lvgl_sys::lv_obj_clear_flag(icon_box, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);

                let icon_label = lvgl_sys::lv_label_create(icon_box);
                lvgl_sys::lv_obj_set_style_text_color(
                    icon_label,
                    lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                    0,
                );
                lvgl_sys::lv_obj_align(
                    icon_label,
                    lvgl_sys::LV_ALIGN_CENTER as lvgl_sys::lv_align_t,
                    0,
                    0,
                );

                let title_label = lvgl_sys::lv_label_create(card);
                lvgl_sys::lv_obj_set_style_text_color(
                    title_label,
                    lvgl_sys::_LV_COLOR_MAKE(220, 220, 220),
                    0,
                );
                lvgl_sys::lv_obj_set_style_text_align(
                    title_label,
                    lvgl_sys::LV_TEXT_ALIGN_CENTER as lvgl_sys::lv_text_align_t,
                    0,
                );

                app_cards_alt[i] = card;
                app_icon_boxes_alt[i] = icon_box;
                app_icon_labels_alt[i] = icon_label;
                app_title_labels_alt[i] = title_label;
            }

            let app_panel = lvgl_sys::lv_obj_create(root);
            lvgl_sys::lv_obj_set_pos(
                app_panel,
                Self::to_coord(width),
                Self::to_coord(TOP_BAR_HEIGHT),
            );
            lvgl_sys::lv_obj_set_size(
                app_panel,
                Self::to_coord(width),
                Self::to_coord(height - TOP_BAR_HEIGHT),
            );
            lvgl_sys::lv_obj_set_style_bg_color(app_panel, lvgl_sys::_LV_COLOR_MAKE(22, 24, 28), 0);
            lvgl_sys::lv_obj_set_style_bg_opa(app_panel, 255, 0);
            lvgl_sys::lv_obj_set_style_border_width(app_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_top(app_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_bottom(app_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_left(app_panel, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_right(app_panel, 0, 0);
            lvgl_sys::lv_obj_clear_flag(app_panel, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
            lvgl_sys::lv_obj_set_scrollbar_mode(
                app_panel,
                lvgl_sys::LV_SCROLLBAR_MODE_OFF as lvgl_sys::lv_scrollbar_mode_t,
            );

            let app_header_card = lvgl_sys::lv_obj_create(app_panel);
            lvgl_sys::lv_obj_set_pos(app_header_card, Self::to_coord(14), Self::to_coord(12));
            lvgl_sys::lv_obj_set_size(
                app_header_card,
                Self::to_coord(width - 28),
                Self::to_coord(76),
            );
            lvgl_sys::lv_obj_set_style_bg_color(
                app_header_card,
                lvgl_sys::_LV_COLOR_MAKE(45, 62, 92),
                0,
            );
            lvgl_sys::lv_obj_set_style_bg_opa(app_header_card, 240, 0);
            lvgl_sys::lv_obj_set_style_radius(app_header_card, 12, 0);
            lvgl_sys::lv_obj_set_style_border_width(app_header_card, 0, 0);
            lvgl_sys::lv_obj_clear_flag(app_header_card, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
            lvgl_sys::lv_obj_set_scrollbar_mode(
                app_header_card,
                lvgl_sys::LV_SCROLLBAR_MODE_OFF as lvgl_sys::lv_scrollbar_mode_t,
            );

            let app_badge_label = lvgl_sys::lv_label_create(app_header_card);
            lvgl_sys::lv_obj_set_style_text_color(
                app_badge_label,
                lvgl_sys::_LV_COLOR_MAKE(228, 236, 255),
                0,
            );
            lvgl_sys::lv_obj_set_style_text_font(
                app_badge_label,
                &lvgl_sys::lv_font_montserrat_14 as *const _ as *const lvgl_sys::lv_font_t,
                0,
            );
            lvgl_sys::lv_obj_align(
                app_badge_label,
                lvgl_sys::LV_ALIGN_TOP_RIGHT as u8,
                Self::to_coord(-14),
                Self::to_coord(14),
            );

            let app_title_label = lvgl_sys::lv_label_create(app_header_card);
            lvgl_sys::lv_obj_set_style_text_color(
                app_title_label,
                lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                0,
            );
            lvgl_sys::lv_obj_set_style_text_font(
                app_title_label,
                &lvgl_sys::lv_font_montserrat_20 as *const _ as *const lvgl_sys::lv_font_t,
                0,
            );
            lvgl_sys::lv_obj_set_pos(app_title_label, Self::to_coord(14), Self::to_coord(-4));

            let app_subtitle_label = lvgl_sys::lv_label_create(app_header_card);
            lvgl_sys::lv_obj_set_style_text_color(
                app_subtitle_label,
                lvgl_sys::_LV_COLOR_MAKE(200, 200, 200),
                0,
            );
            lvgl_sys::lv_obj_set_style_text_font(
                app_subtitle_label,
                &lvgl_sys::lv_font_montserrat_14 as *const _ as *const lvgl_sys::lv_font_t,
                0,
            );
            lvgl_sys::lv_obj_set_pos(app_subtitle_label, Self::to_coord(14), Self::to_coord(24));
            lvgl_sys::lv_obj_set_width(app_subtitle_label, Self::to_coord(width - 56));

            let mut app_metric_cards = [std::ptr::null_mut(); 2];
            let mut app_metric_titles = [std::ptr::null_mut(); 2];
            let mut app_metric_values = [std::ptr::null_mut(); 2];
            let mut app_metric_bars = [std::ptr::null_mut(); 2];

            for i in 0..2 {
                let card = lvgl_sys::lv_obj_create(app_panel);
                let card_w = ((width - 42) / 2).max(120);
                let x = 14 + i as i32 * (card_w + 14);
                lvgl_sys::lv_obj_set_pos(card, Self::to_coord(x), Self::to_coord(100));
                lvgl_sys::lv_obj_set_size(card, Self::to_coord(card_w), Self::to_coord(76));
                lvgl_sys::lv_obj_set_style_radius(card, 12, 0);
                lvgl_sys::lv_obj_set_style_bg_color(card, lvgl_sys::_LV_COLOR_MAKE(30, 32, 38), 0);
                lvgl_sys::lv_obj_set_style_bg_opa(card, 255, 0);
                lvgl_sys::lv_obj_set_style_border_width(card, 0, 0);
                lvgl_sys::lv_obj_clear_flag(card, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
                lvgl_sys::lv_obj_set_scrollbar_mode(
                    card,
                    lvgl_sys::LV_SCROLLBAR_MODE_OFF as lvgl_sys::lv_scrollbar_mode_t,
                );

                let title = lvgl_sys::lv_label_create(card);
                lvgl_sys::lv_obj_set_style_text_color(
                    title,
                    lvgl_sys::_LV_COLOR_MAKE(140, 148, 160),
                    0,
                );
                lvgl_sys::lv_obj_set_style_text_font(
                    title,
                    &lvgl_sys::lv_font_montserrat_14 as *const _ as *const lvgl_sys::lv_font_t,
                    0,
                );
                lvgl_sys::lv_obj_set_pos(title, Self::to_coord(10), Self::to_coord(-4));

                let value = lvgl_sys::lv_label_create(card);
                lvgl_sys::lv_obj_set_style_text_color(
                    value,
                    lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                    0,
                );
                lvgl_sys::lv_obj_set_style_text_font(
                    value,
                    &lvgl_sys::lv_font_montserrat_20 as *const _ as *const lvgl_sys::lv_font_t,
                    0,
                );
                lvgl_sys::lv_obj_set_pos(value, Self::to_coord(10), Self::to_coord(20));

                let bar = lvgl_sys::lv_bar_create(card);
                lvgl_sys::lv_obj_set_pos(bar, Self::to_coord(10), Self::to_coord(58));
                lvgl_sys::lv_obj_set_size(bar, Self::to_coord(card_w - 20), Self::to_coord(6));
                lvgl_sys::lv_bar_set_range(bar, 0, 100);
                lvgl_sys::lv_bar_set_value(bar, 0, lvgl_sys::lv_anim_enable_t_LV_ANIM_OFF);
                lvgl_sys::lv_obj_set_style_bg_color(bar, lvgl_sys::_LV_COLOR_MAKE(40, 44, 50), 0);
                lvgl_sys::lv_obj_set_style_bg_color(
                    bar,
                    lvgl_sys::_LV_COLOR_MAKE(120, 196, 255),
                    lvgl_sys::LV_PART_INDICATOR,
                );
                lvgl_sys::lv_obj_set_style_radius(bar, 3, 0);

                app_metric_cards[i] = card;
                app_metric_titles[i] = title;
                app_metric_values[i] = value;
                app_metric_bars[i] = bar;
            }

            let app_list_title = lvgl_sys::lv_label_create(app_panel);
            lvgl_sys::lv_obj_set_style_text_color(
                app_list_title,
                lvgl_sys::_LV_COLOR_MAKE(150, 160, 174),
                0,
            );
            lvgl_sys::lv_obj_set_style_text_font(
                app_list_title,
                &lvgl_sys::lv_font_montserrat_14 as *const _ as *const lvgl_sys::lv_font_t,
                0,
            );
            lvgl_sys::lv_obj_set_pos(app_list_title, Self::to_coord(14), Self::to_coord(188));

            let mut app_list_row_cards = [std::ptr::null_mut(); 4];
            let mut app_list_row_keys = [std::ptr::null_mut(); 4];
            let mut app_list_row_vals = [std::ptr::null_mut(); 4];

            let row_height = 36;
            let row_spacing = 8;
            for i in 0..4 {
                let row_card = lvgl_sys::lv_obj_create(app_panel);
                let y = 212 + i as i32 * (row_height + row_spacing);
                lvgl_sys::lv_obj_set_pos(row_card, Self::to_coord(14), Self::to_coord(y));
                lvgl_sys::lv_obj_set_size(
                    row_card,
                    Self::to_coord(width - 28),
                    Self::to_coord(row_height),
                );
                lvgl_sys::lv_obj_set_style_radius(row_card, 8, 0);
                lvgl_sys::lv_obj_set_style_bg_color(
                    row_card,
                    lvgl_sys::_LV_COLOR_MAKE(34, 36, 42),
                    0,
                );
                lvgl_sys::lv_obj_set_style_bg_opa(row_card, 255, 0);
                lvgl_sys::lv_obj_set_style_border_width(row_card, 1, 0);
                lvgl_sys::lv_obj_set_style_border_color(
                    row_card,
                    lvgl_sys::_LV_COLOR_MAKE(34, 36, 42),
                    0,
                );
                lvgl_sys::lv_obj_clear_flag(row_card, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
                lvgl_sys::lv_obj_set_scrollbar_mode(
                    row_card,
                    lvgl_sys::LV_SCROLLBAR_MODE_OFF as lvgl_sys::lv_scrollbar_mode_t,
                );

                let key_label = lvgl_sys::lv_label_create(row_card);
                lvgl_sys::lv_obj_set_style_text_color(
                    key_label,
                    lvgl_sys::_LV_COLOR_MAKE(180, 186, 196),
                    0,
                );
                lvgl_sys::lv_obj_set_style_text_font(
                    key_label,
                    &lvgl_sys::lv_font_montserrat_14 as *const _ as *const lvgl_sys::lv_font_t,
                    0,
                );
                lvgl_sys::lv_label_set_long_mode(
                    key_label,
                    lvgl_sys::LV_LABEL_LONG_CLIP as lvgl_sys::lv_label_long_mode_t,
                );
                lvgl_sys::lv_obj_set_width(key_label, Self::to_coord((width - 60) / 2));
                lvgl_sys::lv_obj_align(
                    key_label,
                    lvgl_sys::LV_ALIGN_LEFT_MID as u8,
                    Self::to_coord(12),
                    0,
                );

                let val_label = lvgl_sys::lv_label_create(row_card);
                lvgl_sys::lv_obj_set_style_text_color(
                    val_label,
                    lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                    0,
                );
                lvgl_sys::lv_obj_set_style_text_font(
                    val_label,
                    &lvgl_sys::lv_font_montserrat_14 as *const _ as *const lvgl_sys::lv_font_t,
                    0,
                );
                lvgl_sys::lv_label_set_long_mode(
                    val_label,
                    lvgl_sys::LV_LABEL_LONG_CLIP as lvgl_sys::lv_label_long_mode_t,
                );
                lvgl_sys::lv_obj_set_width(val_label, Self::to_coord((width - 60) / 2));
                lvgl_sys::lv_obj_align(
                    val_label,
                    lvgl_sys::LV_ALIGN_RIGHT_MID as u8,
                    Self::to_coord(-12),
                    0,
                );

                app_list_row_cards[i] = row_card;
                app_list_row_keys[i] = key_label;
                app_list_row_vals[i] = val_label;
            }

            let app_hint_label = lvgl_sys::lv_label_create(app_panel);
            lvgl_sys::lv_obj_set_style_text_color(
                app_hint_label,
                lvgl_sys::_LV_COLOR_MAKE(170, 174, 182),
                0,
            );
            lvgl_sys::lv_obj_set_pos(
                app_hint_label,
                Self::to_coord(14),
                Self::to_coord(height - TOP_BAR_HEIGHT - 34),
            );
            lvgl_sys::lv_obj_set_width(app_hint_label, Self::to_coord(width - 28));

            let snapshot_layer = lvgl_sys::lv_obj_create(root);
            lvgl_sys::lv_obj_set_pos(snapshot_layer, Self::to_coord(0), Self::to_coord(0));
            lvgl_sys::lv_obj_set_size(
                snapshot_layer,
                Self::to_coord(width),
                Self::to_coord(height),
            );
            lvgl_sys::lv_obj_set_style_bg_opa(snapshot_layer, 0, 0);
            lvgl_sys::lv_obj_set_style_border_width(snapshot_layer, 0, 0);
            lvgl_sys::lv_obj_set_style_radius(snapshot_layer, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_top(snapshot_layer, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_bottom(snapshot_layer, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_left(snapshot_layer, 0, 0);
            lvgl_sys::lv_obj_set_style_pad_right(snapshot_layer, 0, 0);
            lvgl_sys::lv_obj_clear_flag(snapshot_layer, lvgl_sys::LV_OBJ_FLAG_SCROLLABLE);
            lvgl_sys::lv_obj_add_flag(snapshot_layer, lvgl_sys::LV_OBJ_FLAG_HIDDEN);

            let snapshot_img_primary = lvgl_sys::lv_img_create(snapshot_layer);
            lvgl_sys::lv_obj_add_flag(snapshot_img_primary, lvgl_sys::LV_OBJ_FLAG_ADV_HITTEST);
            lvgl_sys::lv_img_set_antialias(snapshot_img_primary, false);
            lvgl_sys::lv_obj_set_pos(
                snapshot_img_primary,
                Self::to_coord(0),
                Self::to_coord(TOP_BAR_HEIGHT),
            );

            let snapshot_img_secondary = lvgl_sys::lv_img_create(snapshot_layer);
            lvgl_sys::lv_obj_add_flag(snapshot_img_secondary, lvgl_sys::LV_OBJ_FLAG_ADV_HITTEST);
            lvgl_sys::lv_img_set_antialias(snapshot_img_secondary, false);
            lvgl_sys::lv_obj_add_flag(snapshot_img_secondary, lvgl_sys::LV_OBJ_FLAG_HIDDEN);
            lvgl_sys::lv_obj_set_pos(
                snapshot_img_secondary,
                Self::to_coord(width),
                Self::to_coord(TOP_BAR_HEIGHT),
            );

            self.ui = Some(LvglUiObjects {
                debug_panel,
                debug_label,
                status_label,
                clock_label,
                page_label,
                launcher_panel,
                launcher_panel_alt,
                app_panel,
                app_header_card,
                app_badge_label,
                app_title_label,
                app_subtitle_label,
                app_metric_cards,
                app_metric_titles,
                app_metric_values,
                app_metric_bars,
                app_list_title,
                app_list_row_cards,
                app_list_row_keys,
                app_list_row_vals,
                app_hint_label,
                branding_label,
                branding_label_alt,
                app_cards,
                app_cards_alt,
                app_icon_boxes,
                app_icon_boxes_alt,
                app_icon_labels,
                app_icon_labels_alt,
                app_title_labels,
                app_title_labels_alt,
                snapshot_layer,
                snapshot_img_primary,
                snapshot_img_secondary,
            });
        }
    }

    fn update_launcher_panel(
        &self,
        page_idx: usize,
        selected: Option<(usize, usize)>,
        branding_label: *mut lvgl_sys::lv_obj_t,
        app_cards: &[*mut lvgl_sys::lv_obj_t; 8],
        app_icon_boxes: &[*mut lvgl_sys::lv_obj_t; 8],
        app_icon_labels: &[*mut lvgl_sys::lv_obj_t; 8],
        app_title_labels: &[*mut lvgl_sys::lv_obj_t; 8],
    ) {
        let p = page(page_idx);
        let panel_h = (self.height as i32 - TOP_BAR_HEIGHT - 20).max(120);
        let panel_w = self.width as i32 - 40;
        let is_home = page_idx == 0;

        unsafe {
            if is_home {
                lvgl_sys::lv_obj_clear_flag(branding_label, lvgl_sys::LV_OBJ_FLAG_HIDDEN);
            } else {
                lvgl_sys::lv_obj_add_flag(branding_label, lvgl_sys::LV_OBJ_FLAG_HIDDEN);
            }
        }

        let col_gap = 20;
        let row_gap = 25;
        let cols = p.cols.max(1) as i32;
        let cell_w = (panel_w - (cols - 1) * col_gap) / cols;
        let cell_h = 140;

        for idx in 0..8 {
            let row = idx / 4;
            let col = idx % 4;
            let card = app_cards[idx];
            let icon_box = app_icon_boxes[idx];
            let title_label = app_title_labels[idx];
            let icon_label = app_icon_labels[idx];

            if row < p.rows {
                if let Some(app) = app_at(page_idx, row, col) {
                    let spec = app_spec(app);
                    let is_selected = selected == Some((row, col));
                    let x = 20 + col as i32 * (cell_w + col_gap);
                    let mut y = 20 + row as i32 * (cell_h + row_gap);

                    if is_home {
                        y = panel_h - cell_h - 40;
                    }

                    let mut icon_size = (cell_h - 25).min(cell_w).min(80);
                    if is_selected {
                        icon_size += 14;
                    }

                    unsafe {
                        lvgl_sys::lv_obj_set_pos(card, Self::to_coord(x), Self::to_coord(y));
                        lvgl_sys::lv_obj_set_size(
                            card,
                            Self::to_coord(cell_w),
                            Self::to_coord(cell_h),
                        );
                        lvgl_sys::lv_obj_set_size(
                            icon_box,
                            Self::to_coord(icon_size),
                            Self::to_coord(icon_size),
                        );
                        lvgl_sys::lv_obj_align(
                            icon_box,
                            lvgl_sys::LV_ALIGN_TOP_MID as lvgl_sys::lv_align_t,
                            0,
                            0,
                        );
                        lvgl_sys::lv_obj_set_style_bg_color(
                            icon_box,
                            lvgl_sys::_LV_COLOR_MAKE(spec.accent.0, spec.accent.1, spec.accent.2),
                            0,
                        );
                        lvgl_sys::lv_obj_set_style_bg_opa(icon_box, 255, 0);
                        lvgl_sys::lv_obj_set_style_text_color(
                            title_label,
                            lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                            0,
                        );
                        lvgl_sys::lv_obj_align(
                            title_label,
                            lvgl_sys::LV_ALIGN_BOTTOM_MID as lvgl_sys::lv_align_t,
                            0,
                            0,
                        );
                        Self::set_label_text(title_label, spec.title);
                        Self::set_label_text(icon_label, spec.icon_text);

                        if is_selected {
                            lvgl_sys::lv_obj_set_style_border_width(icon_box, 4, 0);
                            lvgl_sys::lv_obj_set_style_border_color(
                                icon_box,
                                lvgl_sys::_LV_COLOR_MAKE(255, 255, 255),
                                0,
                            );
                            lvgl_sys::lv_obj_set_style_border_opa(icon_box, 255, 0);
                            lvgl_sys::lv_obj_set_style_text_font(
                                title_label,
                                &lvgl_sys::lv_font_montserrat_20 as *const _
                                    as *const lvgl_sys::lv_font_t,
                                0,
                            );
                        } else {
                            lvgl_sys::lv_obj_set_style_border_width(icon_box, 0, 0);
                            lvgl_sys::lv_obj_set_style_outline_width(icon_box, 0, 0);
                            lvgl_sys::lv_obj_set_style_text_font(
                                title_label,
                                &lvgl_sys::lv_font_montserrat_14 as *const _
                                    as *const lvgl_sys::lv_font_t,
                                0,
                            );
                        }
                    }
                    continue;
                }
            }

            unsafe {
                lvgl_sys::lv_obj_set_pos(
                    card,
                    Self::to_coord(self.width as i32 + 100),
                    Self::to_coord(self.height as i32 + 100),
                );
            }
        }
    }

    fn update_launcher(&self, frame: &UiFrame, ui: &LvglUiObjects) {
        self.update_launcher_panel(
            frame.launcher_page,
            Some((frame.selected_row, frame.selected_col)),
            ui.branding_label,
            &ui.app_cards,
            &ui.app_icon_boxes,
            &ui.app_icon_labels,
            &ui.app_title_labels,
        );
    }

    fn update_app_page(&self, frame: &UiFrame, ui: &LvglUiObjects, app: AppId) {
        let data = self.app_template_data(frame, app);

        unsafe {
            lvgl_sys::lv_obj_set_style_bg_color(
                ui.app_header_card,
                lvgl_sys::_LV_COLOR_MAKE(data.accent.0, data.accent.1, data.accent.2),
                0,
            );

            for card in ui.app_metric_cards {
                lvgl_sys::lv_obj_set_style_border_width(card, 1, 0);
                lvgl_sys::lv_obj_set_style_border_color(
                    card,
                    lvgl_sys::_LV_COLOR_MAKE(
                        data.accent.0 / 2,
                        data.accent.1 / 2,
                        data.accent.2 / 2,
                    ),
                    0,
                );
            }
            for bar in ui.app_metric_bars {
                lvgl_sys::lv_obj_set_style_bg_color(
                    bar,
                    lvgl_sys::_LV_COLOR_MAKE(data.accent.0, data.accent.1, data.accent.2),
                    lvgl_sys::LV_PART_INDICATOR,
                );
            }
        }

        Self::set_label_text(ui.app_badge_label, &data.badge);
        Self::set_label_text(ui.app_title_label, &data.title);
        Self::set_label_text(ui.app_subtitle_label, &data.subtitle);

        for i in 0..2 {
            Self::set_label_text(ui.app_metric_titles[i], &data.metric_titles[i]);
            Self::set_label_text(ui.app_metric_values[i], &data.metric_values[i]);
            unsafe {
                lvgl_sys::lv_bar_set_value(
                    ui.app_metric_bars[i],
                    data.metric_progress[i].into(),
                    lvgl_sys::lv_anim_enable_t_LV_ANIM_OFF,
                );
            }
        }

        Self::set_label_text(ui.app_list_title, &data.list_title);
        for i in 0..4 {
            let line = &data.list_lines[i];
            let mut is_selected = false;
            let mut text = line.as_str();

            text = text.trim_start();
            if text.starts_with(">") {
                is_selected = true;
                text = text[1..].trim_start();
            }

            let row_feedback = data
                .feedback
                .as_ref()
                .filter(|feedback| match &feedback.target {
                    UiFeedbackTarget::SelectedListRow => is_selected,
                    UiFeedbackTarget::FieldId(field_id) => {
                        data.list_row_ids[i].as_deref() == Some(field_id.as_str())
                    }
                    UiFeedbackTarget::Page => false,
                });

            if text.is_empty() {
                unsafe {
                    lvgl_sys::lv_obj_add_flag(
                        ui.app_list_row_cards[i],
                        lvgl_sys::LV_OBJ_FLAG_HIDDEN,
                    );
                    lvgl_sys::lv_obj_set_style_translate_x(ui.app_list_row_cards[i], 0, 0);
                }
            } else {
                unsafe {
                    lvgl_sys::lv_obj_clear_flag(
                        ui.app_list_row_cards[i],
                        lvgl_sys::LV_OBJ_FLAG_HIDDEN,
                    );
                    let (border_r, border_g, border_b, bg_r, bg_g, bg_b) =
                        if let Some(feedback) = row_feedback {
                            let accent = Self::feedback_accent(feedback);
                            (
                                accent.0,
                                accent.1,
                                accent.2,
                                accent.0 / 4,
                                accent.1 / 4,
                                accent.2 / 4,
                            )
                        } else if is_selected {
                            (
                                data.accent.0,
                                data.accent.1,
                                data.accent.2,
                                data.accent.0 / 3,
                                data.accent.1 / 3,
                                data.accent.2 / 3,
                            )
                        } else {
                            (34, 36, 42, 34, 36, 42)
                        };
                    lvgl_sys::lv_obj_set_style_translate_x(
                        ui.app_list_row_cards[i],
                        Self::to_coord(
                            row_feedback
                                .map(Self::feedback_row_offset)
                                .unwrap_or_default(),
                        ),
                        0,
                    );
                    if is_selected || row_feedback.is_some() {
                        lvgl_sys::lv_obj_set_style_border_color(
                            ui.app_list_row_cards[i],
                            lvgl_sys::_LV_COLOR_MAKE(border_r, border_g, border_b),
                            0,
                        );
                        lvgl_sys::lv_obj_set_style_bg_color(
                            ui.app_list_row_cards[i],
                            lvgl_sys::_LV_COLOR_MAKE(bg_r, bg_g, bg_b),
                            0,
                        );
                    } else {
                        lvgl_sys::lv_obj_set_style_border_color(
                            ui.app_list_row_cards[i],
                            lvgl_sys::_LV_COLOR_MAKE(border_r, border_g, border_b),
                            0,
                        );
                        lvgl_sys::lv_obj_set_style_bg_color(
                            ui.app_list_row_cards[i],
                            lvgl_sys::_LV_COLOR_MAKE(bg_r, bg_g, bg_b),
                            0,
                        );
                    }
                }

                let key_color = if let Some(feedback) = row_feedback {
                    let accent = Self::feedback_accent(feedback);
                    if text.contains(':') {
                        Self::color_make(255, 255, 255)
                    } else {
                        Self::color_make(accent.0, accent.1, accent.2)
                    }
                } else if is_selected {
                    Self::color_make(255, 255, 255)
                } else {
                    Self::color_make(180, 186, 196)
                };
                let val_color = if let Some(feedback) = row_feedback {
                    let accent = Self::feedback_accent(feedback);
                    Self::color_make(accent.0, accent.1, accent.2)
                } else {
                    Self::color_make(255, 255, 255)
                };

                if let Some(colon_idx) = text.find(':') {
                    let key = text[..colon_idx].trim();
                    let val = text[colon_idx + 1..].trim();
                    Self::set_label_text(ui.app_list_row_keys[i], key);
                    Self::set_label_text(ui.app_list_row_vals[i], val);
                    unsafe {
                        lvgl_sys::lv_obj_set_style_text_color(
                            ui.app_list_row_keys[i],
                            key_color,
                            0,
                        );
                        lvgl_sys::lv_obj_set_style_text_color(
                            ui.app_list_row_vals[i],
                            val_color,
                            0,
                        );
                        lvgl_sys::lv_obj_clear_flag(
                            ui.app_list_row_vals[i],
                            lvgl_sys::LV_OBJ_FLAG_HIDDEN,
                        );
                        lvgl_sys::lv_obj_align(
                            ui.app_list_row_keys[i],
                            lvgl_sys::LV_ALIGN_LEFT_MID as u8,
                            Self::to_coord(12),
                            0,
                        );
                    }
                } else {
                    Self::set_label_text(ui.app_list_row_keys[i], text);
                    unsafe {
                        lvgl_sys::lv_obj_set_style_text_color(
                            ui.app_list_row_keys[i],
                            key_color,
                            0,
                        );
                        lvgl_sys::lv_obj_add_flag(
                            ui.app_list_row_vals[i],
                            lvgl_sys::LV_OBJ_FLAG_HIDDEN,
                        );
                        lvgl_sys::lv_obj_align(
                            ui.app_list_row_keys[i],
                            lvgl_sys::LV_ALIGN_CENTER as u8,
                            0,
                            0,
                        );
                    }
                }
            }
        }
        if let Some(feedback) = data
            .feedback
            .as_ref()
            .filter(|feedback| feedback.slot == UiFeedbackSlot::AppHint)
        {
            let hint_color = Self::feedback_hint_color(feedback);
            Self::set_label_text(ui.app_hint_label, &feedback.message);
            unsafe {
                lvgl_sys::lv_obj_set_style_text_color(
                    ui.app_hint_label,
                    Self::color_make(hint_color.0, hint_color.1, hint_color.2),
                    0,
                );
            }
        } else {
            Self::set_label_text(ui.app_hint_label, &data.hint);
            unsafe {
                lvgl_sys::lv_obj_set_style_text_color(
                    ui.app_hint_label,
                    Self::color_make(170, 174, 182),
                    0,
                );
            }
        }
    }

    pub(super) fn sync_ui(&mut self, frame: &UiFrame) {
        let Some(ui) = self.ui else {
            return;
        };
        let prev_frame = self.last_synced_frame.clone();
        let prev_frame = prev_frame.as_ref();

        if prev_frame
            .map(|prev| prev.debug != frame.debug)
            .unwrap_or(true)
        {
            Self::set_hidden_if_changed(
                ui.debug_panel,
                !frame.debug.enabled,
                &mut self.debug_overlay_hidden,
            );
            if frame.debug.enabled {
                let cpu = frame
                    .debug
                    .cpu_percent
                    .map(|value| format!("{value}%"))
                    .unwrap_or_else(|| "--".to_string());
                let debug = format!("FPS {}\nCPU {cpu}", frame.debug.fps);
                Self::set_label_text(ui.debug_label, &debug);
            }
        }

        let top_status_feedback = frame
            .interaction_feedback
            .as_ref()
            .filter(|feedback| feedback.slot == UiFeedbackSlot::TopStatusBar);

        if prev_frame
            .map(|prev| {
                prev.status != frame.status
                    || prev
                        .interaction_feedback
                        .as_ref()
                        .map(|feedback| (feedback.seq, feedback.slot))
                        != frame
                            .interaction_feedback
                            .as_ref()
                            .map(|feedback| (feedback.seq, feedback.slot))
            })
            .unwrap_or(true)
        {
            if let Some(feedback) = top_status_feedback {
                let color = Self::feedback_hint_color(feedback);
                Self::set_label_text(ui.status_label, &feedback.message);
                unsafe {
                    lvgl_sys::lv_obj_set_style_text_color(
                        ui.status_label,
                        Self::color_make(color.0, color.1, color.2),
                        0,
                    );
                }
            } else {
                let status = format!(
                    "R {}%  A {}%  S {}%",
                    frame.status.remote_battery_percent,
                    frame.status.aircraft_battery_percent,
                    frame.status.signal_strength_percent,
                );
                Self::set_label_text(ui.status_label, &status);
                unsafe {
                    lvgl_sys::lv_obj_set_style_text_color(
                        ui.status_label,
                        Self::color_make(180, 180, 185),
                        0,
                    );
                }
            }

            let secs = frame.status.unix_time_secs % 86400;
            let clock = format!(
                "{:02}:{:02}:{:02}",
                secs / 3600,
                (secs % 3600) / 60,
                secs % 60
            );
            Self::set_label_text(ui.clock_label, &clock);
        }

        if prev_frame
            .map(|prev| {
                prev.launcher_page != frame.launcher_page
                    || prev
                        .interaction_feedback
                        .as_ref()
                        .map(|feedback| (feedback.seq, feedback.slot))
                        != frame
                            .interaction_feedback
                            .as_ref()
                            .map(|feedback| (feedback.seq, feedback.slot))
            })
            .unwrap_or(true)
        {
            let page_txt = if top_status_feedback.is_some() {
                String::new()
            } else {
                format!("Page {}/{}", frame.launcher_page + 1, PAGE_SPECS.len())
            };
            Self::set_label_text(ui.page_label, &page_txt);
        }

        let hidden_right = self.hidden_right();
        let hidden_left = self.hidden_left();
        let prev_page = self.last_page;
        let prev_launcher_page = self.last_launcher_page;

        if self.last_page.is_none() {
            self.current_launcher_x = 0;
            self.target_launcher_x = 0;
            self.current_app_x = if matches!(frame.page, UiPage::App(_)) {
                0
            } else {
                hidden_right
            };
            self.target_app_x = self.current_app_x;
        } else if prev_page != Some(frame.page) {
            match (prev_page, frame.page) {
                (Some(UiPage::Launcher), UiPage::App(_)) => {
                    self.current_launcher_x = 0;
                    self.target_launcher_x = 0;
                    self.current_app_x = hidden_right;
                    self.target_app_x = 0;
                }
                (Some(UiPage::App(_)), UiPage::Launcher) => {
                    self.current_launcher_x = 0;
                    self.target_launcher_x = 0;
                    self.current_app_x = self.drag_offset_x.unwrap_or(0).max(0);
                    self.target_app_x = hidden_right;
                }
                _ => {}
            }
        } else if matches!(frame.page, UiPage::Launcher)
            && prev_launcher_page != frame.launcher_page
        {
            self.launcher_transition_from = Some(prev_launcher_page);
            self.current_launcher_x = if frame.launcher_page > prev_launcher_page {
                (self.current_launcher_x + hidden_right).clamp(0, hidden_right)
            } else {
                (self.current_launcher_x - hidden_right).clamp(hidden_left, 0)
            };
            self.target_launcher_x = 0;
        }

        let snapshot_scene = self.snapshot_scene(frame, prev_page);
        if snapshot_scene.is_some() {
            super::super::debug_log(&format!(
                "snapshot_scene={:?} drag_offset={:?} transition_from={:?}",
                snapshot_scene, self.drag_offset_x, self.launcher_transition_from
            ));
        }

        match frame.page {
            UiPage::Launcher => {
                let mut alt_page = None;
                let mut alt_x = hidden_right;
                let mut launcher_main_page = frame.launcher_page;
                let mut launcher_main_selected = Some((frame.selected_row, frame.selected_col));
                let mut transition_completed = false;
                if let Some(drag_x) = self.drag_offset_x {
                    self.current_launcher_x = drag_x.clamp(hidden_left, hidden_right);
                    self.target_launcher_x = self.current_launcher_x;
                    if drag_x < 0 && frame.launcher_page + 1 < PAGE_SPECS.len() {
                        alt_page = Some(frame.launcher_page + 1);
                        alt_x = self.current_launcher_x + hidden_right;
                    } else if drag_x > 0 && frame.launcher_page > 0 {
                        alt_page = Some(frame.launcher_page - 1);
                        alt_x = self.current_launcher_x - hidden_right;
                    }
                } else {
                    self.target_launcher_x = 0;
                    Self::animate_launcher_axis(
                        &mut self.current_launcher_x,
                        self.target_launcher_x,
                    );
                    if let Some(from_page) = self.launcher_transition_from {
                        launcher_main_page = from_page;
                        launcher_main_selected = None;
                        alt_page = Some(frame.launcher_page);
                        alt_x = self.current_launcher_x;
                        if self.current_launcher_x == self.target_launcher_x {
                            self.launcher_transition_from = None;
                            launcher_main_page = frame.launcher_page;
                            launcher_main_selected = Some((frame.selected_row, frame.selected_col));
                            alt_page = None;
                            alt_x = hidden_right;
                            transition_completed = true;
                        }
                    }
                }
                Self::animate_app_axis(&mut self.current_app_x, self.target_app_x);

                let launcher_changed = prev_frame
                    .map(|prev| {
                        prev.page != frame.page
                            || prev.launcher_page != frame.launcher_page
                            || prev.selected_row != frame.selected_row
                            || prev.selected_col != frame.selected_col
                    })
                    .unwrap_or(true)
                    || transition_completed;
                if launcher_changed {
                    self.update_launcher_panel(
                        launcher_main_page,
                        launcher_main_selected,
                        ui.branding_label,
                        &ui.app_cards,
                        &ui.app_icon_boxes,
                        &ui.app_icon_labels,
                        &ui.app_title_labels,
                    );
                }
                if let Some(page_idx) = alt_page.filter(|_| self.last_alt_launcher_page != alt_page)
                {
                    let alt_selected = if self.drag_offset_x.is_none()
                        && self.launcher_transition_from.is_some()
                        && page_idx == frame.launcher_page
                    {
                        Some((frame.selected_row, frame.selected_col))
                    } else {
                        None
                    };
                    self.update_launcher_panel(
                        page_idx,
                        alt_selected,
                        ui.branding_label_alt,
                        &ui.app_cards_alt,
                        &ui.app_icon_boxes_alt,
                        &ui.app_icon_labels_alt,
                        &ui.app_title_labels_alt,
                    );
                }
                self.last_alt_launcher_page = alt_page;

                match snapshot_scene {
                    Some(scene @ SnapshotScene::LauncherDrag { .. }) => {
                        if self.ensure_snapshot_scene(frame, &ui, scene) {
                            self.layout_snapshot_launcher(&ui, alt_page.map(|_| alt_x));
                        } else {
                            self.teardown_snapshot_scene(&ui);
                            Self::set_obj_pos_if_changed(
                                ui.launcher_panel,
                                &mut self.last_launcher_panel_pos,
                                if self.launcher_transition_from.is_some()
                                    && self.drag_offset_x.is_none()
                                {
                                    0
                                } else {
                                    self.current_launcher_x
                                },
                                TOP_BAR_HEIGHT,
                            );
                            Self::set_obj_pos_if_changed(
                                ui.app_panel,
                                &mut self.last_app_panel_pos,
                                self.current_app_x,
                                TOP_BAR_HEIGHT,
                            );
                            Self::set_obj_pos_if_changed(
                                ui.launcher_panel_alt,
                                &mut self.last_launcher_panel_alt_pos,
                                alt_page.map(|_| alt_x).unwrap_or(hidden_right),
                                TOP_BAR_HEIGHT,
                            );
                        }
                    }
                    _ => {
                        self.teardown_snapshot_scene(&ui);
                        Self::set_obj_pos_if_changed(
                            ui.launcher_panel,
                            &mut self.last_launcher_panel_pos,
                            if self.launcher_transition_from.is_some()
                                && self.drag_offset_x.is_none()
                            {
                                0
                            } else {
                                self.current_launcher_x
                            },
                            TOP_BAR_HEIGHT,
                        );
                        Self::set_obj_pos_if_changed(
                            ui.app_panel,
                            &mut self.last_app_panel_pos,
                            self.current_app_x,
                            TOP_BAR_HEIGHT,
                        );
                        Self::set_obj_pos_if_changed(
                            ui.launcher_panel_alt,
                            &mut self.last_launcher_panel_alt_pos,
                            alt_page.map(|_| alt_x).unwrap_or(hidden_right),
                            TOP_BAR_HEIGHT,
                        );
                    }
                }
            }
            UiPage::App(app) => {
                if let Some(drag_x) = self.drag_offset_x {
                    self.current_app_x = drag_x.clamp(0, hidden_right);
                    self.target_app_x = self.current_app_x;
                } else {
                    self.target_app_x = 0;
                    Self::animate_app_axis(&mut self.current_app_x, self.target_app_x);
                }

                if prev_frame
                    .map(|prev| {
                        prev.page != frame.page
                            || prev.launcher_page != frame.launcher_page
                            || prev.selected_row != frame.selected_row
                            || prev.selected_col != frame.selected_col
                    })
                    .unwrap_or(true)
                {
                    self.update_launcher(frame, &ui);
                }
                if prev_frame.map(|prev| prev != frame).unwrap_or(true) {
                    self.update_app_page(frame, &ui, app);
                }
                self.teardown_snapshot_scene(&ui);
                Self::set_obj_pos_if_changed(
                    ui.launcher_panel,
                    &mut self.last_launcher_panel_pos,
                    0,
                    TOP_BAR_HEIGHT,
                );
                Self::set_obj_pos_if_changed(
                    ui.launcher_panel_alt,
                    &mut self.last_launcher_panel_alt_pos,
                    hidden_right,
                    TOP_BAR_HEIGHT,
                );
                Self::set_obj_pos_if_changed(
                    ui.app_panel,
                    &mut self.last_app_panel_pos,
                    self.current_app_x,
                    TOP_BAR_HEIGHT,
                );
                self.last_alt_launcher_page = None;
            }
        }

        self.last_synced_frame = Some(frame.clone());
        self.last_page = Some(frame.page);
        self.last_launcher_page = frame.launcher_page;
    }
}
