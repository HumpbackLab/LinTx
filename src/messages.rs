use crate::config::ModelConfig;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdcRawMsg {
    pub value: [i16; 4],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RcInputRawMsg {
    pub axes: [i16; 4],
    pub switch_3pos: [u8; 4],
    pub switch_2pos: [bool; 2],
    pub buttons: u32,
    pub switches_present: bool,
}

#[cfg(test)]
mod rc_input_tests {
    use super::*;

    #[test]
    fn test_rc_input_raw_defaults_to_safe_switches() {
        let msg = RcInputRawMsg::default();

        assert_eq!(msg.axes, [0; 4]);
        assert_eq!(msg.switch_3pos, [0; 4]);
        assert_eq!(msg.switch_2pos, [false; 2]);
        assert_eq!(msg.buttons, 0);
        assert!(!msg.switches_present);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputSource {
    Adc,
    Stm32Serial,
    CrsfRcIn,
    JoyDev,
    Mock,
    #[default]
    Unknown,
}

impl InputSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Adc => "ADC",
            Self::Stm32Serial => "STM32 Serial",
            Self::CrsfRcIn => "CRSF RC In",
            Self::JoyDev => "joydev",
            Self::Mock => "Mock",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputHealth {
    #[default]
    Idle,
    Running,
    Error,
}

impl InputHealth {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Running => "Running",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputFrameMsg {
    pub source: InputSource,
    pub channels: Vec<i16>,
}

impl InputFrameMsg {
    pub fn channel_value(&self, index: usize) -> i16 {
        self.channels.get(index).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputStatusMsg {
    pub source: InputSource,
    pub health: InputHealth,
    pub detail: String,
    pub channel_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElrsFeedbackMsg {
    pub connected: bool,
    pub signal_strength_percent: Option<u8>,
    pub aircraft_battery_percent: Option<u8>,
    pub last_update_unix_secs: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFeedbackSeverity {
    Error,
    Success,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiFeedbackTarget {
    SelectedListRow,
    FieldId(String),
    Page,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFeedbackMotion {
    None,
    ShakeX,
    Pulse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFeedbackSlot {
    TopStatusBar,
    AppHint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiInteractionFeedback {
    pub seq: u32,
    pub severity: UiFeedbackSeverity,
    pub target: UiFeedbackTarget,
    pub motion: UiFeedbackMotion,
    pub slot: UiFeedbackSlot,
    pub message: String,
    pub ttl_ms: u32,
}

pub fn publish_input_frame(
    frame_tx: &rpos::channel::Sender<InputFrameMsg>,
    legacy_adc_tx: Option<&rpos::channel::Sender<AdcRawMsg>>,
    source: InputSource,
    channels: &[i16],
) {
    frame_tx.send(InputFrameMsg {
        source,
        channels: channels.to_vec(),
    });

    if let Some(tx) = legacy_adc_tx {
        let mut value = [0i16; 4];
        for (index, channel) in channels.iter().copied().enumerate().take(value.len()) {
            value[index] = channel;
        }
        tx.send(AdcRawMsg { value });
    }
}

pub fn publish_rc_input_raw(
    raw_tx: &rpos::channel::Sender<RcInputRawMsg>,
    frame_tx: &rpos::channel::Sender<InputFrameMsg>,
    legacy_adc_tx: Option<&rpos::channel::Sender<AdcRawMsg>>,
    source: InputSource,
    input: RcInputRawMsg,
) {
    raw_tx.send(input);
    publish_input_frame(frame_tx, legacy_adc_tx, source, &input.axes);
}

pub fn publish_input_status(
    status_tx: &rpos::channel::Sender<InputStatusMsg>,
    source: InputSource,
    health: InputHealth,
    detail: impl Into<String>,
    channel_count: usize,
) {
    status_tx.send(InputStatusMsg {
        source,
        health,
        detail: detail.into(),
        channel_count,
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemStatusMsg {
    pub remote_battery_percent: u8,
    pub aircraft_battery_percent: u8,
    pub signal_strength_percent: u8,
    pub unix_time_secs: u64,
}

impl Default for SystemStatusMsg {
    fn default() -> Self {
        Self {
            remote_battery_percent: 100,
            aircraft_battery_percent: 100,
            signal_strength_percent: 100,
            unix_time_secs: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemConfigMsg {
    pub backlight_percent: u8,
    pub sound_percent: u8,
}

impl Default for SystemConfigMsg {
    fn default() -> Self {
        Self {
            backlight_percent: 70,
            sound_percent: 60,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActiveModelMsg {
    pub model: ModelConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElrsParamEntry {
    pub id: String,
    pub label: String,
    pub value: String,
    pub selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElrsStateMsg {
    pub connected: bool,
    pub rf_output_enabled: bool,
    pub link_active: bool,
    pub busy: bool,
    pub can_leave: bool,
    pub path: String,
    pub editor_active: bool,
    pub editor_label: String,
    pub editor_buffer: String,
    pub editor_cursor: usize,
    pub module_name: String,
    pub device_name: String,
    pub version: String,
    pub packet_rate: String,
    pub telemetry_ratio: String,
    pub tx_power: String,
    pub status_text: String,
    pub wifi_running: bool,
    pub selected_idx: usize,
    pub params: Vec<ElrsParamEntry>,
}

impl Default for ElrsStateMsg {
    fn default() -> Self {
        Self {
            connected: false,
            rf_output_enabled: false,
            link_active: false,
            busy: false,
            can_leave: true,
            path: "/".to_string(),
            editor_active: false,
            editor_label: String::new(),
            editor_buffer: String::new(),
            editor_cursor: 0,
            module_name: "ELRS".to_string(),
            device_name: "Not Connected".to_string(),
            version: "--".to_string(),
            packet_rate: "--".to_string(),
            telemetry_ratio: "--".to_string(),
            tx_power: "100mW".to_string(),
            status_text: "ELRS service not started".to_string(),
            wifi_running: false,
            selected_idx: 0,
            params: vec![
                ElrsParamEntry {
                    id: "rf_output".to_string(),
                    label: "RF Output".to_string(),
                    value: "OFF".to_string(),
                    selectable: true,
                },
                ElrsParamEntry {
                    id: "wifi_manual".to_string(),
                    label: "Module WiFi".to_string(),
                    value: "OFF".to_string(),
                    selectable: true,
                },
                ElrsParamEntry {
                    id: "bind".to_string(),
                    label: "Bind".to_string(),
                    value: "READY".to_string(),
                    selectable: true,
                },
                ElrsParamEntry {
                    id: "tx_power".to_string(),
                    label: "TX Power".to_string(),
                    value: "100mW".to_string(),
                    selectable: true,
                },
                ElrsParamEntry {
                    id: "tx_max_power".to_string(),
                    label: "TX Max Power".to_string(),
                    value: "1000mW".to_string(),
                    selectable: true,
                },
                ElrsParamEntry {
                    id: "link_state".to_string(),
                    label: "Link State".to_string(),
                    value: "RF OFF".to_string(),
                    selectable: false,
                },
                ElrsParamEntry {
                    id: "signal".to_string(),
                    label: "Signal".to_string(),
                    value: "--".to_string(),
                    selectable: false,
                },
                ElrsParamEntry {
                    id: "aircraft_battery".to_string(),
                    label: "Aircraft Battery".to_string(),
                    value: "--".to_string(),
                    selectable: false,
                },
                ElrsParamEntry {
                    id: "telemetry_fresh".to_string(),
                    label: "Telemetry Fresh".to_string(),
                    value: "stale".to_string(),
                    selectable: false,
                },
                ElrsParamEntry {
                    id: "feedback".to_string(),
                    label: "Feedback".to_string(),
                    value: "waiting".to_string(),
                    selectable: false,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElrsCommandMsg {
    Refresh,
    Back,
    SelectPrev,
    SelectNext,
    ValueDec,
    ValueInc,
    Activate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbGamepadCommandMsg {
    Start,
    Stop,
    Toggle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsbGamepadDriverCommandMsg {
    pub generation: u32,
    pub stop: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsbGamepadStateMsg {
    pub hid_ready: bool,
    pub running: bool,
    pub report_count: u32,
    pub generation: u32,
    pub detail: String,
}

#[rpos::ctor::ctor]
fn register() {
    rpos::msg::add_message::<AdcRawMsg>("adc_raw");
    rpos::msg::add_message::<RcInputRawMsg>("rc_input_raw");
    rpos::msg::add_message::<InputFrameMsg>("input_frame");
    rpos::msg::add_message::<InputStatusMsg>("input_status");
    rpos::msg::add_message::<ElrsFeedbackMsg>("elrs_feedback");
    rpos::msg::add_message::<SystemStatusMsg>("system_status");
    rpos::msg::add_message::<SystemConfigMsg>("system_config");
    rpos::msg::add_message::<ActiveModelMsg>("active_model");
    rpos::msg::add_message::<ElrsStateMsg>("elrs_state");
    rpos::msg::add_message::<ElrsCommandMsg>("elrs_cmd");
    rpos::msg::add_message::<UsbGamepadCommandMsg>("usb_gamepad_cmd");
    rpos::msg::add_message::<UsbGamepadDriverCommandMsg>("usb_gamepad_driver_cmd");
    rpos::msg::add_message::<UsbGamepadStateMsg>("usb_gamepad_state");
    rpos::msg::add_message::<UiInteractionFeedback>("ui_interaction_feedback");
}

#[cfg(test)]
mod tests {
    use super::{InputFrameMsg, InputHealth, InputSource, InputStatusMsg};

    #[test]
    fn test_input_frame_channel_value_defaults_to_zero() {
        let frame = InputFrameMsg {
            source: InputSource::Mock,
            channels: vec![100, 200],
        };

        assert_eq!(frame.channel_value(0), 100);
        assert_eq!(frame.channel_value(1), 200);
        assert_eq!(frame.channel_value(2), 0);
    }

    #[test]
    fn test_input_status_default_is_idle_unknown() {
        let status = InputStatusMsg::default();

        assert_eq!(status.source, InputSource::Unknown);
        assert_eq!(status.health, InputHealth::Idle);
        assert_eq!(status.channel_count, 0);
        assert!(status.detail.is_empty());
    }
}
