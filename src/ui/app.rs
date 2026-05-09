use std::time::Duration;

use rpos::{
    channel::Sender,
    msg::{get_new_rx_of_message, get_new_tx_of_message},
};

use crate::{
    config::store,
    messages::{
        ActiveModelMsg, ElrsCommandMsg, ElrsFeedbackMsg, ElrsStateMsg, InputFrameMsg,
        InputStatusMsg, SystemConfigMsg, SystemStatusMsg, UiInteractionFeedback,
        UsbGamepadCommandMsg, UsbGamepadStateMsg,
    },
    mixer::MixerOutMsg,
    ui::feedback::UiFeedbackController,
};

use super::{
    apps::{self, UiAppContext},
    backend::LvglBackend,
    catalog::{app_at, page, PAGE_SPECS},
    input::UiInputEvent,
    keyboard::{KeyboardAction, KeyboardResult},
    model::{AppId, UiDebugStats, UiFrame, UiModelEntry, UiPage},
};

const UI_ACTIVE_ANIMATION_WINDOW: Duration = Duration::from_millis(280);
const UI_MAX_IDLE_SLEEP: Duration = Duration::from_millis(80);
const UI_DEBUG_SAMPLE_WINDOW: Duration = Duration::from_secs(1);

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
struct CpuSnapshot {
    process_ticks: u64,
    total_ticks: u64,
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct LinuxCpuUsageSampler {
    prev: Option<CpuSnapshot>,
    cpu_count: u64,
}

#[cfg(target_os = "linux")]
impl LinuxCpuUsageSampler {
    fn new() -> Self {
        Self {
            prev: None,
            cpu_count: std::thread::available_parallelism()
                .map(|count| count.get() as u64)
                .unwrap_or(1),
        }
    }

    fn sample(&mut self) -> Option<u16> {
        let snapshot = Self::read_snapshot()?;
        let usage = self.prev.and_then(|prev| {
            let proc_delta = snapshot.process_ticks.saturating_sub(prev.process_ticks);
            let total_delta = snapshot.total_ticks.saturating_sub(prev.total_ticks);
            if total_delta == 0 {
                None
            } else {
                Some(
                    ((proc_delta as f64) * 100.0 * self.cpu_count as f64 / total_delta as f64)
                        .round()
                        .clamp(0.0, u16::MAX as f64) as u16,
                )
            }
        });
        self.prev = Some(snapshot);
        usage
    }

    fn read_snapshot() -> Option<CpuSnapshot> {
        let process_ticks = Self::read_process_ticks()?;
        let total_ticks = Self::read_total_ticks()?;
        Some(CpuSnapshot {
            process_ticks,
            total_ticks,
        })
    }

    fn read_process_ticks() -> Option<u64> {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let close_paren = stat.rfind(')')?;
        let fields: Vec<&str> = stat.get(close_paren + 2..)?.split_whitespace().collect();
        let utime = fields.get(11)?.parse::<u64>().ok()?;
        let stime = fields.get(12)?.parse::<u64>().ok()?;
        Some(utime.saturating_add(stime))
    }

    fn read_total_ticks() -> Option<u64> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let cpu_line = stat.lines().find(|line| line.starts_with("cpu "))?;
        cpu_line
            .split_whitespace()
            .skip(1)
            .try_fold(0u64, |acc, field| {
                field
                    .parse::<u64>()
                    .ok()
                    .map(|value| acc.saturating_add(value))
            })
    }
}

#[cfg(not(target_os = "linux"))]
#[derive(Default)]
struct LinuxCpuUsageSampler;

#[cfg(not(target_os = "linux"))]
impl LinuxCpuUsageSampler {
    fn new() -> Self {
        Self
    }

    fn sample(&mut self) -> Option<u16> {
        None
    }
}

struct UiPerfSampler {
    enabled: bool,
    window_start: std::time::Instant,
    frames_in_window: u32,
    cpu_sampler: LinuxCpuUsageSampler,
}

impl UiPerfSampler {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            window_start: std::time::Instant::now(),
            frames_in_window: 0,
            cpu_sampler: LinuxCpuUsageSampler::new(),
        }
    }

    fn initial_stats(&self) -> UiDebugStats {
        UiDebugStats {
            enabled: self.enabled,
            fps: 0,
            cpu_percent: None,
        }
    }

    fn on_render(&mut self, stats: &mut UiDebugStats, now: std::time::Instant) {
        if !self.enabled {
            return;
        }

        self.frames_in_window = self.frames_in_window.saturating_add(1);
        let elapsed = now.saturating_duration_since(self.window_start);
        if elapsed < UI_DEBUG_SAMPLE_WINDOW {
            return;
        }

        let secs = elapsed.as_secs_f64();
        let fps = if secs > 0.0 {
            ((self.frames_in_window as f64) / secs).round() as u16
        } else {
            0
        };
        stats.enabled = true;
        stats.fps = fps;
        stats.cpu_percent = self.cpu_sampler.sample();
        self.window_start = now;
        self.frames_in_window = 0;
    }
}

pub struct UiApp {
    frame: UiFrame,
    feedback_controller: UiFeedbackController,
}

impl UiApp {
    fn update_field<T: PartialEq>(slot: &mut T, value: T) -> bool {
        if *slot == value {
            false
        } else {
            *slot = value;
            true
        }
    }

    pub fn new() -> Self {
        let mut app = Self {
            frame: UiFrame::default(),
            feedback_controller: UiFeedbackController::default(),
        };
        app.frame.debug.enabled = super::debug_overlay_enabled();
        app.reload_models();
        app
    }

    fn reload_models(&mut self) {
        if let Err(err) = store::ensure_default_layout() {
            super::debug_log(&format!("ensure_default_layout failed: {err}"));
            return;
        }

        let models = match store::list_models() {
            Ok(models) => models,
            Err(err) => {
                super::debug_log(&format!("list_models failed: {err}"));
                return;
            }
        };

        self.frame.model_entries = models
            .iter()
            .map(|model| UiModelEntry {
                id: model.id.clone(),
                name: model.name.clone(),
                protocol: model.output.protocol.display_name().to_string(),
            })
            .collect();

        let active_model_id = store::load_radio_config()
            .map(|radio| radio.active_model)
            .unwrap_or_default();

        self.frame.model_active_idx = self
            .frame
            .model_entries
            .iter()
            .position(|entry| entry.id == active_model_id)
            .unwrap_or(0);

        if self.frame.model_entries.is_empty() {
            self.frame.model_focus_idx = 0;
            self.frame.model_active_idx = 0;
        } else {
            self.frame.model_focus_idx = self
                .frame
                .model_focus_idx
                .min(self.frame.model_entries.len().saturating_sub(1));
            self.frame.model_active_idx = self
                .frame
                .model_active_idx
                .min(self.frame.model_entries.len().saturating_sub(1));
        }
    }

    fn publish_active_model(&self, active_model_tx: &Sender<ActiveModelMsg>) {
        match store::load_active_model() {
            Ok(model) => active_model_tx.send(ActiveModelMsg { model }),
            Err(err) => super::debug_log(&format!("load_active_model failed: {err}")),
        }
    }

    fn normalize_selection(&mut self) {
        let p = page(self.frame.launcher_page);
        if self.frame.selected_row >= p.rows {
            self.frame.selected_row = p.rows.saturating_sub(1);
        }
        if self.frame.selected_col >= p.cols {
            self.frame.selected_col = p.cols.saturating_sub(1);
        }
        while app_at(
            self.frame.launcher_page,
            self.frame.selected_row,
            self.frame.selected_col,
        )
        .is_none()
            && self.frame.selected_col > 0
        {
            self.frame.selected_col -= 1;
        }
    }

    fn move_selection_vertical(&mut self, dr: isize) {
        let p = page(self.frame.launcher_page);
        let nr = self.frame.selected_row as isize + dr;
        if nr < 0 {
            return;
        }
        let nr = nr as usize;
        if nr >= p.rows {
            return;
        }

        if app_at(self.frame.launcher_page, nr, self.frame.selected_col).is_some() {
            self.frame.selected_row = nr;
        }
    }

    fn move_left(&mut self) {
        if self.frame.selected_col > 0 {
            self.frame.selected_col -= 1;
            return;
        }

        if self.frame.launcher_page > 0 {
            self.frame.launcher_page -= 1;
            let p = page(self.frame.launcher_page);
            self.frame.selected_col = p.cols.saturating_sub(1);
            self.normalize_selection();
        }
    }

    fn move_right(&mut self) {
        let p = page(self.frame.launcher_page);
        if self.frame.selected_col + 1 < p.cols
            && app_at(
                self.frame.launcher_page,
                self.frame.selected_row,
                self.frame.selected_col + 1,
            )
            .is_some()
        {
            self.frame.selected_col += 1;
            return;
        }

        if self.frame.launcher_page + 1 < PAGE_SPECS.len() {
            self.frame.launcher_page += 1;
            self.frame.selected_col = 0;
            self.normalize_selection();
        }
    }

    fn apply_event_in_app(
        &mut self,
        app: AppId,
        event: UiInputEvent,
        config_tx: &Sender<SystemConfigMsg>,
        active_model_tx: &Sender<ActiveModelMsg>,
        elrs_cmd_tx: &Sender<ElrsCommandMsg>,
        ui_feedback_tx: &Sender<UiInteractionFeedback>,
        usb_gamepad_cmd_tx: &Sender<UsbGamepadCommandMsg>,
    ) {
        let ctx = UiAppContext {
            config_tx,
            active_model_tx,
            elrs_cmd_tx,
            ui_feedback_tx,
            usb_gamepad_cmd_tx,
        };
        apps::handle_event(app, &mut self.frame, event, &ctx);
    }

    fn apply_event(
        &mut self,
        event: UiInputEvent,
        config_tx: &Sender<SystemConfigMsg>,
        active_model_tx: &Sender<ActiveModelMsg>,
        elrs_cmd_tx: &Sender<ElrsCommandMsg>,
        ui_feedback_tx: &Sender<UiInteractionFeedback>,
        usb_gamepad_cmd_tx: &Sender<UsbGamepadCommandMsg>,
    ) -> bool {
        if self.frame.keyboard.is_some() {
            return self.apply_keyboard_event(
                event,
                config_tx,
                active_model_tx,
                elrs_cmd_tx,
                ui_feedback_tx,
                usb_gamepad_cmd_tx,
            );
        }

        match event {
            UiInputEvent::Quit => return false,
            UiInputEvent::Back => {
                if let UiPage::App(app) = self.frame.page {
                    if apps::should_intercept_back(app, &self.frame) {
                        self.apply_event_in_app(
                            app,
                            event,
                            config_tx,
                            active_model_tx,
                            elrs_cmd_tx,
                            ui_feedback_tx,
                            usb_gamepad_cmd_tx,
                        );
                    } else {
                        self.frame.page = UiPage::Launcher;
                    }
                } else {
                    self.frame.page = UiPage::Launcher;
                }
            }
            UiInputEvent::Open => {
                if self.frame.page == UiPage::Launcher {
                    if let Some(app) = app_at(
                        self.frame.launcher_page,
                        self.frame.selected_row,
                        self.frame.selected_col,
                    ) {
                        self.frame.page = UiPage::App(app);
                    }
                } else if let UiPage::App(app) = self.frame.page {
                    self.apply_event_in_app(
                        app,
                        event,
                        config_tx,
                        active_model_tx,
                        elrs_cmd_tx,
                        ui_feedback_tx,
                        usb_gamepad_cmd_tx,
                    );
                }
            }
            UiInputEvent::Left => {
                if self.frame.page == UiPage::Launcher {
                    self.move_left();
                } else if let UiPage::App(app) = self.frame.page {
                    self.apply_event_in_app(
                        app,
                        event,
                        config_tx,
                        active_model_tx,
                        elrs_cmd_tx,
                        ui_feedback_tx,
                        usb_gamepad_cmd_tx,
                    );
                }
            }
            UiInputEvent::Right => {
                if self.frame.page == UiPage::Launcher {
                    self.move_right();
                } else if let UiPage::App(app) = self.frame.page {
                    self.apply_event_in_app(
                        app,
                        event,
                        config_tx,
                        active_model_tx,
                        elrs_cmd_tx,
                        ui_feedback_tx,
                        usb_gamepad_cmd_tx,
                    );
                }
            }
            UiInputEvent::Up | UiInputEvent::Down => {
                if self.frame.page == UiPage::Launcher {
                    self.move_selection_vertical(if event == UiInputEvent::Up { -1 } else { 1 });
                } else if let UiPage::App(app) = self.frame.page {
                    self.apply_event_in_app(
                        app,
                        event,
                        config_tx,
                        active_model_tx,
                        elrs_cmd_tx,
                        ui_feedback_tx,
                        usb_gamepad_cmd_tx,
                    );
                }
            }
            UiInputEvent::PagePrev => {
                if self.frame.page == UiPage::Launcher && self.frame.launcher_page > 0 {
                    self.frame.launcher_page -= 1;
                    self.normalize_selection();
                }
            }
            UiInputEvent::PageNext => {
                if self.frame.page == UiPage::Launcher
                    && self.frame.launcher_page + 1 < PAGE_SPECS.len()
                {
                    self.frame.launcher_page += 1;
                    self.normalize_selection();
                }
            }
            UiInputEvent::KeyboardTap { .. } | UiInputEvent::KeyboardSubmit => {}
        }
        true
    }

    fn apply_keyboard_event(
        &mut self,
        event: UiInputEvent,
        config_tx: &Sender<SystemConfigMsg>,
        active_model_tx: &Sender<ActiveModelMsg>,
        elrs_cmd_tx: &Sender<ElrsCommandMsg>,
        ui_feedback_tx: &Sender<UiInteractionFeedback>,
        usb_gamepad_cmd_tx: &Sender<UsbGamepadCommandMsg>,
    ) -> bool {
        let result = {
            let keyboard = self.frame.keyboard.as_mut().expect("keyboard checked");
            match event {
                UiInputEvent::Quit => return false,
                UiInputEvent::Back | UiInputEvent::PagePrev => {
                    keyboard.activate(KeyboardAction::Cancel)
                }
                UiInputEvent::Open => keyboard.activate_selected(),
                UiInputEvent::Left => {
                    keyboard.move_left();
                    KeyboardResult::None
                }
                UiInputEvent::Right => {
                    keyboard.move_right();
                    KeyboardResult::None
                }
                UiInputEvent::Up => {
                    keyboard.move_up();
                    KeyboardResult::None
                }
                UiInputEvent::Down => {
                    keyboard.move_down();
                    KeyboardResult::None
                }
                UiInputEvent::PageNext => KeyboardResult::None,
                UiInputEvent::KeyboardTap { row, col } => {
                    keyboard.select(row, col);
                    keyboard.activate_selected()
                }
                UiInputEvent::KeyboardSubmit => keyboard.activate(KeyboardAction::Submit),
            }
        };

        match result {
            KeyboardResult::None => {}
            KeyboardResult::Cancelled => {
                self.frame.keyboard = None;
            }
            KeyboardResult::Submitted { field, value } => {
                let ctx = UiAppContext {
                    config_tx,
                    active_model_tx,
                    elrs_cmd_tx,
                    ui_feedback_tx,
                    usb_gamepad_cmd_tx,
                };
                let accepted = match self.frame.page {
                    UiPage::App(app) => {
                        apps::handle_keyboard_submit(app, &mut self.frame, field, &value, &ctx)
                    }
                    UiPage::Launcher => false,
                };
                if accepted {
                    self.frame.keyboard = None;
                } else if let Some(keyboard) = self.frame.keyboard.as_mut() {
                    keyboard.mark_invalid();
                }
            }
        }

        true
    }

    fn sync_interaction_feedback(&mut self, now: std::time::Instant) -> bool {
        let next_snapshot = if matches!(self.frame.page, UiPage::App(_)) {
            self.feedback_controller.snapshot(now)
        } else {
            None
        };
        Self::update_field(&mut self.frame.interaction_feedback, next_snapshot)
    }

    pub fn run(&mut self, backend: &mut dyn LvglBackend, fps: u32) {
        super::debug_log(&format!("UiApp::run start fps={fps}"));
        let mut status_rx = get_new_rx_of_message::<SystemStatusMsg>("system_status").unwrap();
        let mut config_rx = get_new_rx_of_message::<SystemConfigMsg>("system_config").unwrap();
        let mut input_status_rx = get_new_rx_of_message::<InputStatusMsg>("input_status").unwrap();
        let mut input_frame_rx = get_new_rx_of_message::<InputFrameMsg>("input_frame").unwrap();
        let mut elrs_feedback_rx =
            get_new_rx_of_message::<ElrsFeedbackMsg>("elrs_feedback").unwrap();
        let mut mixer_out_rx = get_new_rx_of_message::<MixerOutMsg>("mixer_out").unwrap();
        let mut elrs_rx = get_new_rx_of_message::<ElrsStateMsg>("elrs_state").unwrap();
        let mut usb_gamepad_state_rx =
            get_new_rx_of_message::<UsbGamepadStateMsg>("usb_gamepad_state").unwrap();
        let mut injected_input_rx =
            get_new_rx_of_message::<UiInputEvent>("ui_input_event").unwrap();
        let config_tx = get_new_tx_of_message::<SystemConfigMsg>("system_config").unwrap();
        let active_model_tx = get_new_tx_of_message::<ActiveModelMsg>("active_model").unwrap();
        let elrs_cmd_tx = get_new_tx_of_message::<ElrsCommandMsg>("elrs_cmd").unwrap();
        let usb_gamepad_cmd_tx =
            get_new_tx_of_message::<UsbGamepadCommandMsg>("usb_gamepad_cmd").unwrap();
        let ui_feedback_tx =
            get_new_tx_of_message::<UiInteractionFeedback>("ui_interaction_feedback").unwrap();
        let mut ui_feedback_rx =
            get_new_rx_of_message::<UiInteractionFeedback>("ui_interaction_feedback").unwrap();

        let elrs_state_tx = get_new_tx_of_message::<ElrsStateMsg>("elrs_state").unwrap();
        elrs_state_tx.send(ElrsStateMsg::default());

        self.reload_models();
        self.publish_active_model(&active_model_tx);

        let frame_time = Duration::from_millis((1000 / fps.max(1)) as u64);
        let idle_sleep = UI_MAX_IDLE_SLEEP.min(frame_time.saturating_mul(2));
        let mut frame_idx: u64 = 0;
        let mut active_until = std::time::Instant::now() + UI_ACTIVE_ANIMATION_WINDOW;
        let mut last_render = std::time::Instant::now()
            .checked_sub(frame_time)
            .unwrap_or_else(std::time::Instant::now);
        let mut perf_sampler = UiPerfSampler::new(super::debug_overlay_enabled());
        self.frame.debug = perf_sampler.initial_stats();

        backend.init();
        super::debug_log("backend.init done");

        loop {
            let loop_start = std::time::Instant::now();
            let mut dirty = false;

            while let Some(status) = status_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.status, status);
            }

            while let Some(cfg) = config_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.config, cfg);
            }

            while let Some(input_status) = input_status_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.input_status, input_status);
            }

            while let Some(input_frame) = input_frame_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.input_frame, input_frame);
            }

            while let Some(feedback) = elrs_feedback_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.elrs_feedback, feedback);
            }

            while let Some(mixer_out) = mixer_out_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.mixer_out, mixer_out);
            }

            while let Some(elrs) = elrs_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.elrs, elrs);
            }

            while let Some(usb_gamepad) = usb_gamepad_state_rx.try_read() {
                dirty |= Self::update_field(&mut self.frame.usb_gamepad, usb_gamepad);
            }

            while let Some(feedback) = ui_feedback_rx.try_read() {
                self.feedback_controller
                    .submit(feedback, std::time::Instant::now());
                dirty = true;
            }

            if self.frame.cloud_connected
                && self.frame.status.unix_time_secs
                    >= self.frame.cloud_last_sync_secs.saturating_add(5)
            {
                self.frame.cloud_last_sync_secs = self.frame.status.unix_time_secs;
                dirty = true;
            }

            while let Some(evt) = backend.poll_event() {
                if !self.apply_event(
                    evt,
                    &config_tx,
                    &active_model_tx,
                    &elrs_cmd_tx,
                    &ui_feedback_tx,
                    &usb_gamepad_cmd_tx,
                ) {
                    backend.shutdown();
                    return;
                }
                dirty = true;
            }

            while let Some(evt) = injected_input_rx.try_read() {
                if !self.apply_event(
                    evt,
                    &config_tx,
                    &active_model_tx,
                    &elrs_cmd_tx,
                    &ui_feedback_tx,
                    &usb_gamepad_cmd_tx,
                ) {
                    backend.shutdown();
                    return;
                }
                dirty = true;
            }

            let now = std::time::Instant::now();
            dirty |= self.sync_interaction_feedback(now);
            if dirty {
                active_until = now + UI_ACTIVE_ANIMATION_WINDOW;
            }

            let should_render = dirty
                || now < active_until
                || now.saturating_duration_since(last_render) >= idle_sleep;

            if should_render {
                perf_sampler.on_render(&mut self.frame.debug, now);
                backend.render(&self.frame);
                last_render = std::time::Instant::now();
                frame_idx = frame_idx.saturating_add(1);
                if super::debug_enabled() && frame_idx % 120 == 0 {
                    super::debug_log(&format!(
                        "render heartbeat frame={} page={:?} launcher_page={} selection=({}, {}) active={}",
                        frame_idx,
                        self.frame.page,
                        self.frame.launcher_page,
                        self.frame.selected_row,
                        self.frame.selected_col,
                        now < active_until
                    ));
                }
            }

            let sleep_for = if std::time::Instant::now() < active_until {
                frame_time.saturating_sub(loop_start.elapsed())
            } else {
                idle_sleep
            };
            std::thread::sleep(sleep_for);
        }
    }
}
