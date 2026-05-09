use crate::{
    messages::{
        ElrsFeedbackMsg, ElrsStateMsg, InputFrameMsg, InputStatusMsg, SystemConfigMsg,
        SystemStatusMsg, UsbGamepadStateMsg,
    },
    mixer::MixerOutMsg,
    ui::feedback::UiFeedbackSnapshot,
    ui::keyboard::{KeyboardField, KeyboardOverlay},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppId {
    System,
    Control,
    Models,
    Cloud,
    UsbGamepad,
    Trainer,
    Scripts,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiPage {
    Launcher,
    App(AppId),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiModelEntry {
    pub id: String,
    pub name: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiDebugStats {
    pub enabled: bool,
    pub fps: u16,
    pub cpu_percent: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiFrame {
    pub page: UiPage,
    pub launcher_page: usize,
    pub selected_row: usize,
    pub selected_col: usize,
    pub status: SystemStatusMsg,
    pub config: SystemConfigMsg,
    pub input_status: InputStatusMsg,
    pub input_frame: InputFrameMsg,
    pub elrs_feedback: ElrsFeedbackMsg,
    pub interaction_feedback: Option<UiFeedbackSnapshot>,
    pub mixer_out: MixerOutMsg,
    pub model_entries: Vec<UiModelEntry>,
    pub model_focus_idx: usize,
    pub model_active_idx: usize,
    pub cloud_connected: bool,
    pub cloud_last_sync_secs: u64,
    pub elrs: ElrsStateMsg,
    pub usb_gamepad: UsbGamepadStateMsg,
    pub debug: UiDebugStats,
    pub keyboard: Option<KeyboardOverlay>,
    pub keyboard_armed_field: Option<KeyboardField>,
}

impl Default for UiFrame {
    fn default() -> Self {
        Self {
            page: UiPage::Launcher,
            launcher_page: 0,
            selected_row: 0,
            selected_col: 0,
            status: SystemStatusMsg::default(),
            config: SystemConfigMsg::default(),
            input_status: InputStatusMsg::default(),
            input_frame: InputFrameMsg::default(),
            elrs_feedback: ElrsFeedbackMsg::default(),
            interaction_feedback: None,
            mixer_out: MixerOutMsg::default(),
            model_entries: Vec::new(),
            model_focus_idx: 0,
            model_active_idx: 0,
            cloud_connected: false,
            cloud_last_sync_secs: 0,
            elrs: ElrsStateMsg::default(),
            usb_gamepad: UsbGamepadStateMsg::default(),
            debug: UiDebugStats::default(),
            keyboard: None,
            keyboard_armed_field: None,
        }
    }
}
