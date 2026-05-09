use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use crc::{Crc, CRC_8_DVB_S2};
use crsf::{PacketAddress, RawPacket};
use rpos::{
    msg::{get_new_rx_of_message, get_new_tx_of_message},
    pthread_scheduler::SchedulePthread,
    thread_logln,
};
use serialport::{DataBits, FlowControl, Parity, StopBits};

use crate::{
    client_process_args,
    config::{store, ElrsUiConfig},
    elrs::{ElrsOperation, ElrsOperationStatus, ElrsProtocolRuntime, CRSF_SYNC},
    messages::{
        ActiveModelMsg, ElrsCommandMsg, ElrsFeedbackMsg, ElrsStateMsg, SystemStatusMsg,
        UiFeedbackMotion, UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget,
        UiInteractionFeedback,
    },
    mixer::MixerOutMsg,
};

const CRSF_FRAME_BATTERY_ID: u8 = 0x08;
const CRSF_FRAME_LINK_ID: u8 = 0x14;
const CRSF_FRAME_LINK_RX_ID: u8 = 0x1C;
const CRSF_MAX_PACKET_SIZE: usize = 66;
const CRSF_CRC: Crc<u8> = Crc::<u8>::new(&CRC_8_DVB_S2);
const REOPEN_DELAY: Duration = Duration::from_millis(500);
const TELEMETRY_STALE_AFTER: Duration = Duration::from_secs(2);
const SERIAL_IO_TIMEOUT: Duration = Duration::from_millis(100);
const WRITE_TIMEOUT_LOG_EVERY: u32 = 50;
const ELRS_STATE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const LINK_ESTABLISH_TIMEOUT: Duration = Duration::from_secs(5);
const BIND_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
const CRSF_RC_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const WIFI_EXIT_SILENCE_INTERVAL: Duration = Duration::from_secs(2);
const DEFAULT_RF_UART: &str = "/dev/ttyS2";
const DEFAULT_RF_LOG_PATH: &str = "/tmp/lintx-elrs/rf_link_service.log";
const DEFAULT_BIND_PHRASE: &str = "654321";
const ELRS_POWER_LEVELS_MW: [u16; 6] = [10, 25, 100, 250, 500, 1000];
const EDITOR_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789-_";

macro_rules! rf_logln {
    ($($arg:tt)*) => {{
        let message = format!($($arg)*);
        thread_logln!("{}", message);
        append_rf_log_line(&message);
    }};
}

#[derive(Parser)]
#[command(name = "rf_link_service", about = "Unified ELRS/CRSF RF link service", long_about = None)]
struct Cli {
    #[arg(short, long, default_value_t = 115200)]
    baudrate: u32,

    #[arg(default_value = DEFAULT_RF_UART)]
    dev_name: String,
}

fn new_rc_channel_packet(channel_vals: &[u16; 16]) -> RawPacket {
    let chn = crsf::RcChannels(*channel_vals);
    let packet = crsf::Packet::RcChannels(chn);
    packet.into_raw(PacketAddress::Transmitter)
}

#[inline]
fn mixer_out_to_crsf(value: u16) -> u16 {
    (value as u32
        * (crsf::RcChannels::CHANNEL_VALUE_MAX - crsf::RcChannels::CHANNEL_VALUE_MIN) as u32
        / 10000
        + crsf::RcChannels::CHANNEL_VALUE_MIN as u32) as u16
}

fn mixer_channels_to_crsf(msg: &MixerOutMsg) -> [u16; 16] {
    let mut channels = [crsf::RcChannels::CHANNEL_VALUE_MIN; 16];
    for (index, value) in msg.channels.iter().copied().enumerate() {
        channels[index] = mixer_out_to_crsf(value);
    }
    channels
}

#[cfg(test)]
mod rc_channel_tests {
    use super::*;

    #[test]
    fn test_mixer_out_to_crsf_handles_endpoints_and_midpoint() {
        assert_eq!(mixer_out_to_crsf(0), crsf::RcChannels::CHANNEL_VALUE_MIN);
        assert_eq!(
            mixer_out_to_crsf(10000),
            crsf::RcChannels::CHANNEL_VALUE_MAX
        );

        let midpoint = mixer_out_to_crsf(5000);
        assert!(midpoint >= crsf::RcChannels::CHANNEL_VALUE_MID - 1);
        assert!(midpoint <= crsf::RcChannels::CHANNEL_VALUE_MID + 1);
    }

    #[test]
    fn test_mixer_channels_to_crsf_converts_all_16_channels() {
        let msg = MixerOutMsg {
            channels: [
                0, 5000, 10000, 5000, 10000, 0, 5000, 10000, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        };

        let channels = mixer_channels_to_crsf(&msg);

        assert_eq!(channels[0], crsf::RcChannels::CHANNEL_VALUE_MIN);
        assert_eq!(channels[1], mixer_out_to_crsf(5000));
        assert_eq!(channels[2], crsf::RcChannels::CHANNEL_VALUE_MAX);
        assert_eq!(channels[4], crsf::RcChannels::CHANNEL_VALUE_MAX);
        assert_eq!(channels[15], crsf::RcChannels::CHANNEL_VALUE_MIN);
    }
}

#[derive(Debug, Clone)]
struct EditorState {
    buffer: Vec<u8>,
    cursor: usize,
}

impl EditorState {
    fn new(initial: &str) -> Self {
        let seed = if initial.is_empty() {
            DEFAULT_BIND_PHRASE
        } else {
            initial
        };
        let mut buffer = seed.as_bytes().to_vec();
        sanitize_editor_buffer(&mut buffer);
        if buffer.is_empty() {
            buffer.push(EDITOR_CHARSET[0]);
        }
        Self { buffer, cursor: 0 }
    }

    fn move_cursor(&mut self, delta: isize) {
        if self.buffer.is_empty() {
            self.cursor = 0;
            return;
        }

        if delta.is_negative() {
            self.cursor = self.cursor.saturating_sub(delta.unsigned_abs());
        } else {
            self.cursor = self
                .cursor
                .saturating_add(delta as usize)
                .min(self.buffer.len().saturating_sub(1));
        }
    }

    fn cycle_char(&mut self, delta: isize) {
        if self.buffer.is_empty() {
            self.buffer.push(EDITOR_CHARSET[0]);
            self.cursor = 0;
            return;
        }

        let current = self.buffer[self.cursor];
        let current_idx = char_index(current).unwrap_or(0);
        let len = EDITOR_CHARSET.len() as isize;
        let next_idx = (current_idx as isize + delta).rem_euclid(len) as usize;
        self.buffer[self.cursor] = EDITOR_CHARSET[next_idx];
    }

    fn as_string(&self) -> String {
        String::from_utf8_lossy(&self.buffer).to_string()
    }
}

#[derive(Debug, Clone)]
struct ElrsUiState {
    config: ElrsUiConfig,
    selected_idx: usize,
    bind_active: bool,
    bind_waiting_for_link: bool,
    editor: Option<EditorState>,
    feedback_seq: u32,
    interaction_feedback: Option<UiInteractionFeedback>,
}

impl ElrsUiState {
    const SELECTABLE_PARAM_COUNT: usize = 6;

    fn load() -> Self {
        let mut config = store::load_radio_config()
            .map(|radio| radio.elrs)
            .unwrap_or_default();
        if config.bind_phrase.is_empty() {
            config.bind_phrase = DEFAULT_BIND_PHRASE.to_string();
        }
        Self {
            config,
            selected_idx: 0,
            bind_active: false,
            bind_waiting_for_link: false,
            editor: None,
            feedback_seq: 0,
            interaction_feedback: None,
        }
    }

    fn editor_label(&self) -> &'static str {
        "Bind Phrase"
    }

    fn selected_feedback_target(&self) -> UiFeedbackTarget {
        match self.selected_idx {
            0 => UiFeedbackTarget::FieldId("rf_output".to_string()),
            1 => UiFeedbackTarget::FieldId("wifi_manual".to_string()),
            2 => UiFeedbackTarget::FieldId("bind".to_string()),
            3 => UiFeedbackTarget::FieldId("tx_power".to_string()),
            4 => UiFeedbackTarget::FieldId("tx_max_power".to_string()),
            5 => UiFeedbackTarget::FieldId("bind_phrase".to_string()),
            _ => UiFeedbackTarget::SelectedListRow,
        }
    }

    fn emit_feedback(
        &mut self,
        severity: UiFeedbackSeverity,
        motion: UiFeedbackMotion,
        message: impl Into<String>,
    ) {
        self.feedback_seq = self.feedback_seq.wrapping_add(1);
        self.interaction_feedback = Some(UiInteractionFeedback {
            seq: self.feedback_seq,
            severity,
            target: self.selected_feedback_target(),
            motion,
            slot: UiFeedbackSlot::TopStatusBar,
            message: message.into(),
            ttl_ms: match severity {
                UiFeedbackSeverity::Error => 900,
                UiFeedbackSeverity::Success => 850,
                UiFeedbackSeverity::Busy => 1200,
            },
        });
    }

    fn clear_feedback(&mut self) {
        self.interaction_feedback = None;
    }

    fn take_feedback(&mut self) -> Option<UiInteractionFeedback> {
        self.interaction_feedback.take()
    }

    fn handle_command(
        &mut self,
        cmd: ElrsCommandMsg,
        protocol: &mut ElrsProtocolRuntime,
        connected: bool,
    ) -> String {
        if self.editor.is_some() {
            return self.handle_editor_command(cmd, protocol, connected);
        }

        match cmd {
            ElrsCommandMsg::Back => {
                self.clear_feedback();
                "Back".to_string()
            }
            ElrsCommandMsg::Refresh => self.reload_from_disk(),
            ElrsCommandMsg::SelectPrev => {
                self.clear_feedback();
                self.selected_idx = self.selected_idx.saturating_sub(1);
                self.current_item_status()
            }
            ElrsCommandMsg::SelectNext => {
                self.clear_feedback();
                let max_idx = Self::SELECTABLE_PARAM_COUNT.saturating_sub(1);
                self.selected_idx = self.selected_idx.saturating_add(1).min(max_idx);
                self.current_item_status()
            }
            ElrsCommandMsg::ValueDec => self.adjust_selected(-1, protocol, connected),
            ElrsCommandMsg::ValueInc => self.adjust_selected(1, protocol, connected),
            ElrsCommandMsg::Activate => self.activate_selected(protocol, connected),
            ElrsCommandMsg::SetBindPhrase(value) => {
                self.set_bind_phrase(value, protocol, connected)
            }
        }
    }

    fn handle_editor_command(
        &mut self,
        cmd: ElrsCommandMsg,
        protocol: &mut ElrsProtocolRuntime,
        connected: bool,
    ) -> String {
        if self.editor.is_none() {
            return "Edit unavailable".to_string();
        }

        match cmd {
            ElrsCommandMsg::Back => {
                self.editor = None;
                let message = "Bind phrase edit cancelled".to_string();
                self.emit_feedback(
                    UiFeedbackSeverity::Error,
                    UiFeedbackMotion::ShakeX,
                    message.clone(),
                );
                message
            }
            ElrsCommandMsg::SelectPrev => {
                self.clear_feedback();
                let editor = self.editor.as_mut().expect("editor checked");
                editor.cycle_char(-1);
                format!("Editing bind phrase: {}", editor.as_string())
            }
            ElrsCommandMsg::SelectNext => {
                self.clear_feedback();
                let editor = self.editor.as_mut().expect("editor checked");
                editor.cycle_char(1);
                format!("Editing bind phrase: {}", editor.as_string())
            }
            ElrsCommandMsg::ValueDec => {
                self.clear_feedback();
                let editor = self.editor.as_mut().expect("editor checked");
                editor.move_cursor(-1);
                format!("Cursor {}", editor.cursor.saturating_add(1))
            }
            ElrsCommandMsg::ValueInc => {
                self.clear_feedback();
                let editor = self.editor.as_mut().expect("editor checked");
                editor.move_cursor(1);
                format!("Cursor {}", editor.cursor.saturating_add(1))
            }
            ElrsCommandMsg::Activate => {
                let Some(editor) = self.editor.as_ref() else {
                    return "Edit unavailable".to_string();
                };
                self.set_bind_phrase(editor.as_string(), protocol, connected)
            }
            ElrsCommandMsg::Refresh => {
                self.clear_feedback();
                "Editing bind phrase".to_string()
            }
            ElrsCommandMsg::SetBindPhrase(value) => {
                self.set_bind_phrase(value, protocol, connected)
            }
        }
    }

    fn set_bind_phrase(
        &mut self,
        value: String,
        protocol: &mut ElrsProtocolRuntime,
        connected: bool,
    ) -> String {
        self.config.bind_phrase = value;
        self.editor = None;
        match self.persist() {
            Ok(()) => {
                let message = if self.config.rf_output_enabled && connected {
                    let status = protocol.request(ElrsOperation::SetBindPhrase(
                        self.config.bind_phrase.clone(),
                    ));
                    match status {
                        ElrsOperationStatus::Queued(_) => {
                            "Bind phrase saved and queued".to_string()
                        }
                        _ => format!("Bind phrase saved locally; {}", status.message()),
                    }
                } else {
                    "Bind phrase saved locally".to_string()
                };
                self.emit_feedback(
                    UiFeedbackSeverity::Success,
                    UiFeedbackMotion::Pulse,
                    message.clone(),
                );
                message
            }
            Err(err) => {
                let message = format!("Bind phrase save failed: {err}");
                self.emit_feedback(
                    UiFeedbackSeverity::Error,
                    UiFeedbackMotion::ShakeX,
                    message.clone(),
                );
                message
            }
        }
    }

    fn adjust_selected(
        &mut self,
        delta: isize,
        protocol: &mut ElrsProtocolRuntime,
        connected: bool,
    ) -> String {
        match self.selected_idx {
            0 => {
                self.config.rf_output_enabled = !self.config.rf_output_enabled;
                match self.persist() {
                    Ok(()) => {
                        let message = if self.config.rf_output_enabled {
                            "RF output enabled".to_string()
                        } else {
                            self.bind_waiting_for_link = false;
                            "RF output disabled".to_string()
                        };
                        self.emit_feedback(
                            UiFeedbackSeverity::Success,
                            UiFeedbackMotion::Pulse,
                            message.clone(),
                        );
                        message
                    }
                    Err(err) => {
                        let message = format!("RF output save failed: {err}");
                        self.emit_feedback(
                            UiFeedbackSeverity::Error,
                            UiFeedbackMotion::ShakeX,
                            message.clone(),
                        );
                        message
                    }
                }
            }
            1 => {
                if self.config.wifi_manual_on {
                    // WiFi 已开启 → 关闭：仅清除本地标志，无需 CRSF 命令
                    // （ELRS 模块进入 WiFi 后需断电重启，本身没有 CRSF disable 命令）
                    let status = protocol.request(ElrsOperation::SetWifiManual(false));
                    self.config.wifi_manual_on = false;
                    let _ = self.persist();
                    let message = status.message().to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Success,
                        UiFeedbackMotion::Pulse,
                        message.clone(),
                    );
                    message
                } else {
                    // WiFi 未开启 → 尝试开启：需要 UART 连接才能发送参数命令
                    if !self.config.rf_output_enabled {
                        let message = "WiFi unavailable: enable RF output first".to_string();
                        self.emit_feedback(
                            UiFeedbackSeverity::Error,
                            UiFeedbackMotion::ShakeX,
                            message.clone(),
                        );
                        message
                    } else if !connected {
                        let message = "WiFi unavailable: UART offline".to_string();
                        self.emit_feedback(
                            UiFeedbackSeverity::Error,
                            UiFeedbackMotion::ShakeX,
                            message.clone(),
                        );
                        message
                    } else {
                        let status = protocol.request(ElrsOperation::SetWifiManual(true));
                        // 只有 Queued（命令已入队发送）才真正置 wifi_manual_on。
                        // Busy（参数加载中）和 Unsupported（模块未找到）都不应置位。
                        if matches!(status, ElrsOperationStatus::Queued(_)) {
                            self.config.wifi_manual_on = true;
                            let _ = self.persist();
                        }
                        let message = status.message().to_string();
                        match status {
                            ElrsOperationStatus::Queued(_) | ElrsOperationStatus::Busy(_) => {
                                self.emit_feedback(
                                    UiFeedbackSeverity::Busy,
                                    UiFeedbackMotion::Pulse,
                                    message.clone(),
                                );
                            }
                            ElrsOperationStatus::Unsupported(_) => {
                                self.emit_feedback(
                                    UiFeedbackSeverity::Error,
                                    UiFeedbackMotion::ShakeX,
                                    message.clone(),
                                );
                            }
                        }
                        message
                    }
                }
            }
            2 => {
                if !self.config.rf_output_enabled {
                    let message = "Bind unavailable: enable RF output first".to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    message
                } else if !connected {
                    let message = "Bind unavailable: UART offline".to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    message
                } else {
                    let status = protocol.request(ElrsOperation::EnterBind);
                    self.bind_active = protocol.bind_active();
                    if self.bind_active {
                        self.bind_waiting_for_link = true;
                    }
                    let message = status.message().to_string();
                    match status {
                        ElrsOperationStatus::Queued(_) | ElrsOperationStatus::Busy(_) => {
                            self.emit_feedback(
                                UiFeedbackSeverity::Busy,
                                UiFeedbackMotion::Pulse,
                                message.clone(),
                            );
                        }
                        ElrsOperationStatus::Unsupported(_) => {
                            self.emit_feedback(
                                UiFeedbackSeverity::Error,
                                UiFeedbackMotion::ShakeX,
                                message.clone(),
                            );
                        }
                    }
                    message
                }
            }
            3 => {
                if !self.config.rf_output_enabled {
                    let message = "TX power unavailable: enable RF output first".to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    return message;
                }
                if !connected {
                    let message = "TX power unavailable: UART offline".to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    return message;
                }
                let requested = shift_power_level(self.config.tx_power_mw, delta);
                if requested > self.config.tx_max_power_mw {
                    let message = format!("TX power exceeds max {}mW", self.config.tx_max_power_mw);
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    return message;
                }
                if !protocol.has_current_tx_power_control() {
                    let message =
                        "TX power is read-only on this module (adjust TX max power)".to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    return message;
                }
                let status = protocol.request(ElrsOperation::SetTxPower(requested));
                let message = status.message().to_string();
                match status {
                    ElrsOperationStatus::Queued(_) | ElrsOperationStatus::Busy(_) => {
                        self.emit_feedback(
                            UiFeedbackSeverity::Busy,
                            UiFeedbackMotion::Pulse,
                            message.clone(),
                        );
                        message
                    }
                    ElrsOperationStatus::Unsupported(_) => {
                        self.emit_feedback(
                            UiFeedbackSeverity::Error,
                            UiFeedbackMotion::ShakeX,
                            message.clone(),
                        );
                        message
                    }
                }
            }
            4 => {
                if !self.config.rf_output_enabled {
                    let message = "TX max power unavailable: enable RF output first".to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    return message;
                }
                if !connected {
                    let message = "TX max power unavailable: UART offline".to_string();
                    self.emit_feedback(
                        UiFeedbackSeverity::Error,
                        UiFeedbackMotion::ShakeX,
                        message.clone(),
                    );
                    return message;
                }
                let requested = shift_power_level(self.config.tx_max_power_mw, delta);
                let status = protocol.request(ElrsOperation::SetTxMaxPower(requested));
                let message = status.message().to_string();
                match status {
                    ElrsOperationStatus::Queued(_) | ElrsOperationStatus::Busy(_) => {
                        self.emit_feedback(
                            UiFeedbackSeverity::Busy,
                            UiFeedbackMotion::Pulse,
                            message.clone(),
                        );
                        message
                    }
                    ElrsOperationStatus::Unsupported(_) => {
                        self.emit_feedback(
                            UiFeedbackSeverity::Error,
                            UiFeedbackMotion::ShakeX,
                            message.clone(),
                        );
                        message
                    }
                }
            }
            5 => {
                self.editor = Some(EditorState::new(&self.config.bind_phrase));
                self.clear_feedback();
                "Editing bind phrase".to_string()
            }
            _ => {
                self.clear_feedback();
                self.current_item_status()
            }
        }
    }

    fn activate_selected(&mut self, protocol: &mut ElrsProtocolRuntime, connected: bool) -> String {
        match self.selected_idx {
            0..=4 => self.adjust_selected(1, protocol, connected),
            5 => {
                self.editor = Some(EditorState::new(&self.config.bind_phrase));
                self.clear_feedback();
                "Editing bind phrase".to_string()
            }
            _ => {
                self.clear_feedback();
                self.current_item_status()
            }
        }
    }

    fn current_item_status(&self) -> String {
        match self.selected_idx {
            0 => {
                if self.config.rf_output_enabled {
                    "RF output is ON".to_string()
                } else {
                    "RF output is OFF".to_string()
                }
            }
            1 => {
                if self.config.wifi_manual_on {
                    "WiFi command set to ON".to_string()
                } else {
                    "WiFi command set to OFF".to_string()
                }
            }
            2 => {
                if self.bind_active {
                    "Bind command in progress".to_string()
                } else if self.bind_waiting_for_link {
                    "Waiting for bind verification".to_string()
                } else {
                    "Bind ready".to_string()
                }
            }
            3 => format!("TX power {}mW", self.config.tx_power_mw),
            4 => format!("TX max power {}mW", self.config.tx_max_power_mw),
            5 => "Bind phrase".to_string(),
            _ => String::new(),
        }
    }

    fn reload_from_disk(&mut self) -> String {
        match store::load_radio_config() {
            Ok(radio) => {
                self.config = radio.elrs;
                if self.config.bind_phrase.is_empty() {
                    self.config.bind_phrase = DEFAULT_BIND_PHRASE.to_string();
                }
                self.editor = None;
                self.bind_active = false;
                self.bind_waiting_for_link = false;
                let message = "ELRS config reloaded".to_string();
                self.emit_feedback(
                    UiFeedbackSeverity::Success,
                    UiFeedbackMotion::Pulse,
                    message.clone(),
                );
                message
            }
            Err(err) => {
                let message = format!("ELRS config reload failed: {err}");
                self.emit_feedback(
                    UiFeedbackSeverity::Error,
                    UiFeedbackMotion::ShakeX,
                    message.clone(),
                );
                message
            }
        }
    }

    fn persist(&self) -> Result<(), String> {
        let mut radio = store::load_radio_config().map_err(|err| err.to_string())?;
        radio.elrs = self.config.clone();
        store::save_radio_config(&radio).map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone)]
struct LinkMonitorState {
    waiting_for_link_since: Option<Instant>,
    bind_verify_deadline: Option<Instant>,
    last_link_active: bool,
    feedback_text: String,
}

impl Default for LinkMonitorState {
    fn default() -> Self {
        Self {
            waiting_for_link_since: None,
            bind_verify_deadline: None,
            last_link_active: false,
            feedback_text: "RF disabled".to_string(),
        }
    }
}

impl LinkMonitorState {
    fn on_rf_enabled(&mut self, now: Instant) {
        self.waiting_for_link_since.get_or_insert(now);
        self.feedback_text = "waiting receiver link".to_string();
    }

    fn on_rf_disabled(&mut self) {
        self.waiting_for_link_since = None;
        self.bind_verify_deadline = None;
        self.last_link_active = false;
        self.feedback_text = "RF disabled".to_string();
    }

    fn on_bind_requested(&mut self, now: Instant) {
        self.waiting_for_link_since = Some(now);
        self.bind_verify_deadline = Some(now + BIND_VERIFY_TIMEOUT);
        self.feedback_text = "Bind sent, waiting receiver link".to_string();
    }

    fn on_link_active(&mut self) -> Option<String> {
        self.waiting_for_link_since = None;
        self.last_link_active = true;
        if self.bind_verify_deadline.take().is_some() {
            self.feedback_text = "Bind verified by telemetry link".to_string();
            return Some(self.feedback_text.clone());
        }

        if self.feedback_text != "Receiver link active" {
            self.feedback_text = "Receiver link active".to_string();
            return Some(self.feedback_text.clone());
        }
        None
    }

    fn on_link_lost(&mut self, now: Instant) -> Option<String> {
        if self.last_link_active {
            self.last_link_active = false;
            self.waiting_for_link_since = Some(now);
            self.feedback_text = "Receiver link lost".to_string();
            return Some(self.feedback_text.clone());
        }
        None
    }

    fn poll_timeout(&mut self, now: Instant) -> Option<String> {
        if let Some(deadline) = self.bind_verify_deadline {
            if now >= deadline {
                self.bind_verify_deadline = None;
                self.feedback_text = "Bind not verified (no telemetry link)".to_string();
                return Some(self.feedback_text.clone());
            }
        }

        if let Some(since) = self.waiting_for_link_since {
            if now.saturating_duration_since(since) >= LINK_ESTABLISH_TIMEOUT
                && self.feedback_text != "RF enabled, no receiver link"
            {
                self.feedback_text = "RF enabled, no receiver link".to_string();
                return Some(self.feedback_text.clone());
            }
        }

        None
    }

    fn feedback_text(&self) -> String {
        self.feedback_text.clone()
    }
}

fn sanitize_editor_buffer(buffer: &mut Vec<u8>) {
    for ch in buffer.iter_mut() {
        if char_index(*ch).is_none() {
            *ch = EDITOR_CHARSET[0];
        }
    }
    if buffer.len() > 32 {
        buffer.truncate(32);
    }
}

fn char_index(ch: u8) -> Option<usize> {
    EDITOR_CHARSET.iter().position(|candidate| *candidate == ch)
}

fn shift_power_level(current: u16, delta: isize) -> u16 {
    let current = normalize_power_level(current);
    let idx = ELRS_POWER_LEVELS_MW
        .iter()
        .position(|power| *power == current)
        .unwrap_or(0) as isize;
    let next = (idx + delta).clamp(0, ELRS_POWER_LEVELS_MW.len() as isize - 1) as usize;
    ELRS_POWER_LEVELS_MW[next]
}

fn normalize_power_level(raw: u16) -> u16 {
    ELRS_POWER_LEVELS_MW
        .iter()
        .min_by_key(|level| level.abs_diff(raw))
        .copied()
        .unwrap_or(100)
}

fn derive_elrs_model_id(model_id: &str) -> u8 {
    let mut hash = 0x811C9DC5u32;
    for byte in model_id.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    (hash & 0xFF) as u8
}

fn load_active_model_id_for_elrs() -> u8 {
    let model = store::load_active_model().unwrap_or_default();
    derive_elrs_model_id(&model.id)
}

fn write_packet_with_timeout_tolerance(
    dev: &mut dyn serialport::SerialPort,
    payload: &[u8],
) -> std::io::Result<()> {
    let mut sent = 0usize;
    while sent < payload.len() {
        match dev.write(&payload[sent..]) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "serial write returned 0",
                ))
            }
            Ok(n) => {
                sent = sent.saturating_add(n);
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn rf_log_path() -> String {
    std::env::var("LINTX_RF_LOG_PATH").unwrap_or_else(|_| DEFAULT_RF_LOG_PATH.to_string())
}

fn append_rf_log_line(message: &str) {
    let path = rf_log_path();
    if let Some(parent) = Path::new(&path).parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            eprintln!(
                "[rf_link_service] failed to create log directory {}: {}",
                parent.display(),
                err
            );
            return;
        }
    }

    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            let _ = writeln!(file, "[{}] {}", now_unix_secs(), message);
        }
        Err(err) => {
            eprintln!(
                "[rf_link_service] failed to open log file {}: {}",
                path, err
            );
        }
    }
}

fn wifi_exit_silence_active(silent_until: Option<Instant>, now: Instant) -> bool {
    silent_until.map(|deadline| now < deadline).unwrap_or(false)
}

fn rf_link_service_main(argc: u32, argv: *const &str) {
    let args = match client_process_args::<Cli>(argc, argv) {
        Some(v) => v,
        None => return,
    };

    let mut mixer_out_rx = get_new_rx_of_message::<MixerOutMsg>("mixer_out").unwrap();
    let mut elrs_cmd_rx = get_new_rx_of_message::<ElrsCommandMsg>("elrs_cmd").unwrap();
    let mut active_model_rx = get_new_rx_of_message::<ActiveModelMsg>("active_model").unwrap();
    let system_status_tx = get_new_tx_of_message::<SystemStatusMsg>("system_status").unwrap();
    let elrs_state_tx = get_new_tx_of_message::<ElrsStateMsg>("elrs_state").unwrap();
    let elrs_feedback_tx = get_new_tx_of_message::<ElrsFeedbackMsg>("elrs_feedback").unwrap();
    let ui_feedback_tx =
        get_new_tx_of_message::<UiInteractionFeedback>("ui_interaction_feedback").unwrap();

    rf_logln!(
        "rf_link_service start on {} @ {} baud",
        args.dev_name,
        args.baudrate
    );

    SchedulePthread::new_simple(Box::new(move |_| {
        let mut status_state = TelemetryStatusState::default();
        let mut ui_state = ElrsUiState::load();
        let mut protocol = ElrsProtocolRuntime::default();
        protocol.set_model_id(load_active_model_id_for_elrs());
        let mut link_monitor = LinkMonitorState::default();
        let mut serial_buf = Vec::<u8>::new();
        let mut read_buf = [0u8; 256];
        let mut latest_mixer_out: Option<MixerOutMsg> = None;
        let mut last_state = ElrsStateMsg::default();
        let mut last_rc_frame_at: Option<Instant> = None;
        let mut write_timeout_count: u32 = 0;
        let mut last_state_heartbeat_at: Option<Instant> = Some(Instant::now());
        let mut ui_status_text = "RF output disabled".to_string();
        let mut wifi_exit_silent_until: Option<Instant> = None;
        let mut wifi_retry_log_armed = true;
        let mut dev: Option<Box<dyn serialport::SerialPort>> = None;

        loop {
            let now = Instant::now();

            while let Some(model_msg) = active_model_rx.try_read() {
                let model_id = derive_elrs_model_id(&model_msg.model.id);
                protocol.set_model_id(model_id);
                rf_logln!(
                    "rf_link_service active model updated: {} -> ELRS model id {}",
                    model_msg.model.id,
                    model_id
                );
            }

            while let Some(cmd) = elrs_cmd_rx.try_read() {
                let prev_rf_enabled = ui_state.config.rf_output_enabled;
                let prev_wifi_manual_on = ui_state.config.wifi_manual_on;
                let prev_bind_active = protocol.bind_active();
                ui_status_text = ui_state.handle_command(cmd.clone(), &mut protocol, dev.is_some());
                if let Some(feedback) = ui_state.take_feedback() {
                    ui_feedback_tx.send(feedback);
                }
                ui_state.bind_active = protocol.bind_active();

                if !prev_rf_enabled && ui_state.config.rf_output_enabled {
                    link_monitor.on_rf_enabled(now);
                } else if prev_rf_enabled && !ui_state.config.rf_output_enabled {
                    protocol.clear_ephemeral();
                    ui_state.bind_active = false;
                    ui_state.bind_waiting_for_link = false;
                    link_monitor.on_rf_disabled();
                }

                if !prev_bind_active && protocol.bind_active() {
                    link_monitor.on_bind_requested(now);
                }

                if prev_wifi_manual_on && !ui_state.config.wifi_manual_on {
                    wifi_exit_silent_until = Some(now + WIFI_EXIT_SILENCE_INTERVAL);
                    wifi_retry_log_armed = true;
                    rf_logln!(
                        "rf_link_service WiFi exit silence enabled for {} ms",
                        WIFI_EXIT_SILENCE_INTERVAL.as_millis()
                    );
                } else if !prev_wifi_manual_on && ui_state.config.wifi_manual_on {
                    wifi_exit_silent_until = None;
                    wifi_retry_log_armed = true;
                }

                rf_logln!(
                    "rf_link_service command: {:?} -> {} (rf_enabled={} bind_active={})",
                    cmd,
                    ui_status_text,
                    ui_state.config.rf_output_enabled,
                    protocol.bind_active()
                );
            }

            if !ui_state.config.rf_output_enabled {
                if dev.take().is_some() {
                    rf_logln!("rf_link_service UART closed because RF output is disabled");
                }
                protocol.clear_ephemeral();
                ui_state.bind_active = false;
                ui_state.bind_waiting_for_link = false;
                serial_buf.clear();
                link_monitor.on_rf_disabled();
                wifi_exit_silent_until = None;
                wifi_retry_log_armed = true;
                if ui_state.config.wifi_manual_on {
                    ui_state.config.wifi_manual_on = false;
                    let _ = ui_state.persist();
                    rf_logln!("rf_link_service WiFi state cleared on RF disable");
                }
                ui_status_text = "RF output disabled".to_string();
            } else {
                link_monitor.on_rf_enabled(now);
                if dev.is_none() {
                    let serial = serialport::new(&args.dev_name, args.baudrate)
                        .timeout(SERIAL_IO_TIMEOUT)
                        .data_bits(DataBits::Eight)
                        .parity(Parity::None)
                        .stop_bits(StopBits::One)
                        .flow_control(FlowControl::None);
                    match serial.open() {
                        Ok(port) => {
                            rf_logln!(
                                "rf_link_service serial open ok on {} @ {} baud",
                                args.dev_name,
                                args.baudrate
                            );
                            dev = Some(port);
                            protocol.request_refresh();
                            wifi_exit_silent_until = None;
                            wifi_retry_log_armed = true;
                            if ui_state.config.wifi_manual_on {
                                ui_state.config.wifi_manual_on = false;
                                let _ = ui_state.persist();
                                rf_logln!("rf_link_service WiFi state cleared on module reconnect");
                            }
                            serial_buf.clear();
                            ui_status_text = format!(
                                "RF output enabled on {} @ {} baud",
                                args.dev_name, args.baudrate
                            );
                        }
                        Err(err) => {
                            protocol.clear_ephemeral();
                            ui_state.bind_active = false;
                            let err_text = format!("open failed: {}", err);
                            let state = build_elrs_state(
                                false,
                                err_text.clone(),
                                &status_state,
                                &ui_state,
                                &protocol,
                                &link_monitor,
                                &args.dev_name,
                            );
                            elrs_feedback_tx.send(build_feedback(
                                false,
                                err_text.clone(),
                                &status_state,
                            ));
                            if state != last_state {
                                elrs_state_tx.send(state.clone());
                                last_state = state;
                            }
                            rf_logln!("rf_link_service open failed on {}: {}", args.dev_name, err);
                            std::thread::sleep(REOPEN_DELAY);
                            continue;
                        }
                    }
                }
            }

            let mut serial_fault: Option<String> = None;
            if let Some(dev) = dev.as_mut() {
                while let Some(msg) = mixer_out_rx.try_read() {
                    latest_mixer_out = Some(msg);
                }

                let wifi_exit_silence = wifi_exit_silence_active(wifi_exit_silent_until, now);

                if serial_fault.is_none() && !wifi_exit_silence {
                    if let Some(frame) = protocol.poll_outgoing_frame(now) {
                        let is_wifi_start_retry = protocol.is_pending_wifi_start_frame(&frame);
                        if frame.get(2).copied() == Some(0x32)
                            && frame.get(6).copied() == Some(0x05)
                        {
                            rf_logln!(
                                "rf_link_service sending ELRS model id on {}: {:02X?}",
                                args.dev_name,
                                frame
                            );
                        } else if frame.get(2).copied() == Some(0x2A) {
                            rf_logln!(
                                "rf_link_service sending ELRS request settings on {}: {:02X?}",
                                args.dev_name,
                                frame
                            );
                        } else if frame.get(2).copied() == Some(0x2D) {
                            if is_wifi_start_retry {
                                if wifi_retry_log_armed {
                                    rf_logln!(
                                        "rf_link_service sending ELRS WiFi start on {}: {:02X?}",
                                        args.dev_name,
                                        frame
                                    );
                                    wifi_retry_log_armed = false;
                                }
                            } else {
                                rf_logln!(
                                    "rf_link_service sending ELRS param frame type=0x{:02X} on {}: {:02X?}",
                                    frame[2],
                                    args.dev_name,
                                    frame
                                );
                            }
                        } else if frame.get(2).copied() == Some(0x32)
                            && frame.get(5).copied() == Some(0x10)
                            && frame.get(6).copied() == Some(0x01)
                        {
                            let (current, total) = protocol.bind_progress().unwrap_or((1, 1));
                            rf_logln!(
                                "rf_link_service sending bind frame {}/{} on {}: {:02X?}",
                                current,
                                total,
                                args.dev_name,
                                frame
                            );
                        }
                        if let Err(err) = write_packet_with_timeout_tolerance(&mut **dev, &frame) {
                            if err.kind() == std::io::ErrorKind::TimedOut {
                                write_timeout_count = write_timeout_count.saturating_add(1);
                                if write_timeout_count % WRITE_TIMEOUT_LOG_EVERY == 1 {
                                    rf_logln!(
                                        "rf_link_service bind write timeout on {} (count={})",
                                        args.dev_name,
                                        write_timeout_count
                                    );
                                }
                            } else {
                                protocol.clear_ephemeral();
                                ui_state.bind_active = false;
                                rf_logln!("rf_link_service bind write failed: {}", err);
                                serial_fault = Some(format!("bind write error: {}", err));
                            }
                        }
                        ui_state.bind_active = protocol.bind_active();
                        if let Some(status) = protocol.take_status_text() {
                            ui_status_text = status;
                        }
                    }
                }

                let rc_due = last_rc_frame_at
                    .map(|last| now.saturating_duration_since(last) >= CRSF_RC_FRAME_INTERVAL)
                    .unwrap_or(true);
                // WiFi 模式激活时模块已切换到 WiFi，不再处理 CRSF 帧；停发 RC 帧避免无效占用 UART。
                if serial_fault.is_none()
                    && !protocol.bind_active()
                    && !ui_state.config.wifi_manual_on
                    && !wifi_exit_silence
                    && rc_due
                {
                    if let Some(msg) = latest_mixer_out {
                        let crsf_chn_values = mixer_channels_to_crsf(&msg);

                        let raw_packet = new_rc_channel_packet(&crsf_chn_values);
                        if let Err(err) =
                            write_packet_with_timeout_tolerance(&mut **dev, raw_packet.data())
                        {
                            if err.kind() == std::io::ErrorKind::TimedOut {
                                write_timeout_count = write_timeout_count.saturating_add(1);
                                if write_timeout_count % WRITE_TIMEOUT_LOG_EVERY == 1 {
                                    rf_logln!(
                                        "rf_link_service write timeout on {} (count={})",
                                        args.dev_name,
                                        write_timeout_count
                                    );
                                }
                            } else {
                                write_timeout_count = 0;
                                rf_logln!("rf_link_service write failed: {}", err);
                                serial_fault = Some(format!("serial write error: {}", err));
                            }
                        } else {
                            write_timeout_count = 0;
                            last_rc_frame_at = Some(now);
                        }
                    }
                }

                if serial_fault.is_none() {
                    match dev.read(&mut read_buf) {
                        Ok(n) if n > 0 => {
                            serial_buf.extend_from_slice(&read_buf[..n]);
                            for frame in extract_crsf_frames(&mut serial_buf) {
                                match frame.get(2).copied().unwrap_or_default() {
                                    0x29 => rf_logln!(
                                        "rf_link_service received ELRS device info: {:02X?}",
                                        frame
                                    ),
                                    0x2B => {
                                        let field_id = frame.get(5).copied().unwrap_or(0);
                                        let chunks_remain = frame.get(6).copied().unwrap_or(0);
                                        let kind_raw = frame.get(8).copied().unwrap_or(0);
                                        let label_bytes = frame.get(9..).unwrap_or(&[]);
                                        let label_len = label_bytes
                                            .iter()
                                            .position(|&b| b == 0)
                                            .unwrap_or(label_bytes.len().saturating_sub(1));
                                        let label = core::str::from_utf8(&label_bytes[..label_len])
                                            .unwrap_or("?");
                                        // COMMAND(0x0D) 允许 chunks_remain>0；其余类型需要完整数据
                                        let dropped = chunks_remain != 0 && kind_raw != 0x0D;
                                        if kind_raw == 0x0D && !dropped {
                                            let step_raw = frame.get(9 + label_len + 1).copied();
                                            let timeout = frame.get(9 + label_len + 2).copied();
                                            let info_bytes =
                                                frame.get(9 + label_len + 3..).unwrap_or(&[]);
                                            let info_len = info_bytes
                                                .iter()
                                                .position(|&b| b == 0)
                                                .unwrap_or(info_bytes.len().saturating_sub(1));
                                            let info =
                                                core::str::from_utf8(&info_bytes[..info_len])
                                                    .unwrap_or("?");
                                            rf_logln!(
                                                "rf_link_service param command field={:#04X} \
                                                 chunks_rem={} label={:?} step={:#04X} timeout={} info={:?}",
                                                field_id,
                                                chunks_remain,
                                                label,
                                                step_raw.unwrap_or(0),
                                                timeout.unwrap_or(0),
                                                info
                                            );
                                        } else {
                                            rf_logln!(
                                                "rf_link_service param entry field={:#04X} \
                                                 chunks_rem={} kind={:#04X} label={:?} {}",
                                                field_id,
                                                chunks_remain,
                                                kind_raw,
                                                label,
                                                if dropped {
                                                    "[DROPPED-multichunk]"
                                                } else {
                                                    "[parsed]"
                                                }
                                            );
                                        }
                                    }
                                    0x2D => {
                                        let field_id = frame.get(5).copied().unwrap_or(0);
                                        let value = frame.get(6).copied().unwrap_or(0);
                                        rf_logln!(
                                            "rf_link_service received ELRS write-ack field={:#04X} value=0x{:02X} frame={:02X?}",
                                            field_id,
                                            value,
                                            frame
                                        );
                                    }
                                    0x2E => rf_logln!(
                                        "rf_link_service received ELRS link stat: {:02X?}",
                                        frame
                                    ),
                                    _ => {}
                                }
                                protocol.consume_frame(&frame);
                                if let Some(status) = status_state.consume_frame(&frame) {
                                    rf_logln!(
                                        "rf_link_service telemetry frame type=0x{:02X} signal={} battery={} stale={}",
                                            frame.get(2).copied().unwrap_or_default(),
                                            status.signal_strength_percent,
                                            status.aircraft_battery_percent,
                                            status_state.telemetry_is_stale()
                                        );
                                    system_status_tx.send(status);
                                }
                            }
                            if let Some(info) = protocol.device_info() {
                                if info.is_elrs {
                                    if let Some(power) = protocol.select_tx_power_value() {
                                        if let Ok(parsed) = power
                                            .chars()
                                            .filter(|ch| ch.is_ascii_digit())
                                            .collect::<String>()
                                            .parse::<u16>()
                                        {
                                            ui_state.config.tx_power_mw = parsed;
                                        }
                                    }
                                    if let Some(max_power) = protocol.select_tx_max_power_value() {
                                        if let Ok(parsed) = max_power
                                            .chars()
                                            .filter(|ch| ch.is_ascii_digit())
                                            .collect::<String>()
                                            .parse::<u16>()
                                        {
                                            ui_state.config.tx_max_power_mw = parsed;
                                            if ui_state.config.tx_power_mw > parsed {
                                                ui_state.config.tx_power_mw = parsed;
                                            }
                                        }
                                    }
                                    let _ = ui_state.persist();
                                }
                            }
                            if let Some(status) = link_monitor.on_link_active() {
                                ui_state.bind_waiting_for_link = false;
                                ui_status_text = status;
                            }
                        }
                        Ok(_) => {}
                        Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(err) => {
                            protocol.clear_ephemeral();
                            ui_state.bind_active = false;
                            rf_logln!("rf_link_service read failed: {}", err);
                            serial_fault = Some(format!("serial error: {}", err));
                        }
                    }
                }
            } else {
                while mixer_out_rx.try_read().is_some() {}
                latest_mixer_out = None;
                last_rc_frame_at = None;
                wifi_exit_silent_until = None;
                wifi_retry_log_armed = true;
            }

            if let Some(err_text) = serial_fault {
                dev = None;
                ui_status_text = err_text;
                std::thread::sleep(REOPEN_DELAY);
                continue;
            }

            if ui_state.config.rf_output_enabled {
                if status_state.telemetry_is_stale() {
                    if let Some(status) = link_monitor.on_link_lost(now) {
                        ui_status_text = status;
                    } else if let Some(status) = link_monitor.poll_timeout(now) {
                        ui_state.bind_waiting_for_link = false;
                        ui_status_text = status;
                    }
                } else if let Some(status) = link_monitor.on_link_active() {
                    ui_state.bind_waiting_for_link = false;
                    ui_status_text = status;
                }
            }

            elrs_feedback_tx.send(build_feedback(
                !status_state.telemetry_is_stale(),
                link_monitor.feedback_text(),
                &status_state,
            ));

            let state = build_elrs_state(
                dev.is_some(),
                ui_status_text.clone(),
                &status_state,
                &ui_state,
                &protocol,
                &link_monitor,
                &args.dev_name,
            );
            if state != last_state {
                elrs_state_tx.send(state.clone());
                last_state = state.clone();
            }

            let heartbeat_due = last_state_heartbeat_at
                .map(|last| now.saturating_duration_since(last) >= ELRS_STATE_HEARTBEAT_INTERVAL)
                .unwrap_or(true);
            if heartbeat_due {
                elrs_state_tx.send(state.clone());
                last_state = state;
                last_state_heartbeat_at = Some(now);
            }

            std::thread::sleep(Duration::from_millis(2));
        }
    }));
}

#[derive(Debug, Clone, Default)]
struct TelemetryStatusState {
    remote_battery_percent: Option<u8>,
    aircraft_battery_percent: Option<u8>,
    signal_strength_percent: Option<u8>,
    last_telemetry_at: Option<Instant>,
    last_telemetry_unix_secs: Option<u64>,
}

impl TelemetryStatusState {
    fn consume_frame(&mut self, frame: &[u8]) -> Option<SystemStatusMsg> {
        if frame.len() < 5 {
            return None;
        }

        let mut changed = false;

        match frame[2] {
            CRSF_FRAME_LINK_ID => {
                if let Some(rx_quality) = frame.get(5).copied() {
                    self.signal_strength_percent = Some(rx_quality.min(100));
                    changed = true;
                }
            }
            CRSF_FRAME_LINK_RX_ID => {
                if let Some(rx_rssi_percent) = frame.get(4).copied() {
                    self.signal_strength_percent = Some(rx_rssi_percent.min(100));
                    changed = true;
                }
            }
            CRSF_FRAME_BATTERY_ID => {
                if let Some(remaining) = frame.get(10).copied() {
                    self.aircraft_battery_percent = Some(remaining.min(100));
                    changed = true;
                }
            }
            _ => {}
        }

        if !changed {
            return None;
        }

        self.last_telemetry_at = Some(Instant::now());
        self.last_telemetry_unix_secs = Some(now_unix_secs());

        Some(SystemStatusMsg {
            remote_battery_percent: self.remote_battery_percent.unwrap_or(100),
            aircraft_battery_percent: self.aircraft_battery_percent.unwrap_or(100),
            signal_strength_percent: self.signal_strength_percent.unwrap_or(100),
            unix_time_secs: now_unix_secs(),
        })
    }

    fn telemetry_is_stale(&self) -> bool {
        match self.last_telemetry_at {
            Some(last) => Instant::now().saturating_duration_since(last) > TELEMETRY_STALE_AFTER,
            None => true,
        }
    }
}

fn build_elrs_state(
    connected: bool,
    status_text: String,
    telemetry: &TelemetryStatusState,
    ui_state: &ElrsUiState,
    protocol: &ElrsProtocolRuntime,
    link_monitor: &LinkMonitorState,
    dev_name: &str,
) -> ElrsStateMsg {
    let editor_active = ui_state.editor.is_some();
    let link_active = ui_state.config.rf_output_enabled && !telemetry.telemetry_is_stale();
    let module_info = protocol.device_info();
    let module_name = module_info
        .map(|info| info.name.clone())
        .unwrap_or_else(|| "ELRS/CRSF".to_string());
    let version = module_info
        .map(|info| {
            format!(
                "{}.{}.{}",
                (info.sw_version >> 16) & 0xff,
                (info.sw_version >> 8) & 0xff,
                info.sw_version & 0xff
            )
        })
        .unwrap_or_else(|| "--".to_string());
    let link_state = if !ui_state.config.rf_output_enabled {
        "RF OFF".to_string()
    } else if !connected {
        "UART OFFLINE".to_string()
    } else if ui_state.bind_active {
        "BINDING".to_string()
    } else if ui_state.bind_waiting_for_link {
        "VERIFY".to_string()
    } else if link_active {
        "LINKED".to_string()
    } else {
        "SEARCH".to_string()
    };
    let (editor_buffer, editor_cursor) = if let Some(editor) = ui_state.editor.as_ref() {
        (editor.as_string(), editor.cursor)
    } else {
        (String::new(), 0)
    };

    ElrsStateMsg {
        connected,
        rf_output_enabled: ui_state.config.rf_output_enabled,
        link_active,
        busy: ui_state.bind_active || ui_state.bind_waiting_for_link,
        can_leave: !editor_active,
        path: dev_name.to_string(),
        editor_active,
        editor_label: if editor_active {
            ui_state.editor_label().to_string()
        } else {
            String::new()
        },
        editor_buffer,
        editor_cursor,
        module_name,
        device_name: if let Some(info) = module_info {
            format!(
                "{} #{:08X}",
                if connected {
                    "UART Connected"
                } else {
                    "Disconnected"
                },
                info.serial
            )
        } else if connected {
            "UART Connected".to_string()
        } else {
            "Disconnected".to_string()
        },
        version,
        packet_rate: "--".to_string(),
        telemetry_ratio: "--".to_string(),
        tx_power: protocol
            .select_tx_power_value()
            .unwrap_or_else(|| format!("{}mW", ui_state.config.tx_power_mw)),
        status_text,
        wifi_running: ui_state.config.wifi_manual_on,
        selected_idx: ui_state.selected_idx,
        params: vec![
            crate::messages::ElrsParamEntry {
                id: "rf_output".to_string(),
                label: "RF Output".to_string(),
                value: if ui_state.config.rf_output_enabled {
                    "ON".to_string()
                } else {
                    "OFF".to_string()
                },
                selectable: true,
            },
            crate::messages::ElrsParamEntry {
                id: "wifi_manual".to_string(),
                label: "Module WiFi".to_string(),
                value: if ui_state.config.wifi_manual_on {
                    "ON".to_string()
                } else {
                    "OFF".to_string()
                },
                selectable: true,
            },
            crate::messages::ElrsParamEntry {
                id: "bind".to_string(),
                label: "Bind".to_string(),
                value: if ui_state.bind_active {
                    "SENDING".to_string()
                } else if ui_state.bind_waiting_for_link {
                    "VERIFY".to_string()
                } else {
                    "READY".to_string()
                },
                selectable: true,
            },
            crate::messages::ElrsParamEntry {
                id: "tx_power".to_string(),
                label: "TX Power".to_string(),
                value: if protocol.has_current_tx_power_control() {
                    format!("{}mW", ui_state.config.tx_power_mw)
                } else {
                    format!(
                        "{}mW (Read-only, use TX Max Power)",
                        ui_state.config.tx_power_mw
                    )
                },
                selectable: protocol.has_current_tx_power_control(),
            },
            crate::messages::ElrsParamEntry {
                id: "tx_max_power".to_string(),
                label: "TX Max Power".to_string(),
                value: format!("{}mW", ui_state.config.tx_max_power_mw),
                selectable: true,
            },
            crate::messages::ElrsParamEntry {
                id: "bind_phrase".to_string(),
                label: "Bind Phrase".to_string(),
                value: if ui_state.config.bind_phrase.is_empty() {
                    DEFAULT_BIND_PHRASE.to_string()
                } else {
                    ui_state.config.bind_phrase.clone()
                },
                selectable: true,
            },
            crate::messages::ElrsParamEntry {
                id: "link_state".to_string(),
                label: "Link State".to_string(),
                value: link_state,
                selectable: false,
            },
            crate::messages::ElrsParamEntry {
                id: "signal".to_string(),
                label: "Signal".to_string(),
                value: telemetry
                    .signal_strength_percent
                    .map(|v| format!("{}%", v))
                    .unwrap_or_else(|| "--".to_string()),
                selectable: false,
            },
            crate::messages::ElrsParamEntry {
                id: "aircraft_battery".to_string(),
                label: "Aircraft Battery".to_string(),
                value: telemetry
                    .aircraft_battery_percent
                    .map(|v| format!("{}%", v))
                    .unwrap_or_else(|| "--".to_string()),
                selectable: false,
            },
            crate::messages::ElrsParamEntry {
                id: "telemetry_fresh".to_string(),
                label: "Telemetry Fresh".to_string(),
                value: if telemetry.telemetry_is_stale() {
                    "stale".to_string()
                } else {
                    "fresh".to_string()
                },
                selectable: false,
            },
            crate::messages::ElrsParamEntry {
                id: "feedback".to_string(),
                label: "Feedback".to_string(),
                value: link_monitor.feedback_text(),
                selectable: false,
            },
        ],
    }
}

fn build_feedback(
    connected: bool,
    detail: String,
    telemetry: &TelemetryStatusState,
) -> ElrsFeedbackMsg {
    ElrsFeedbackMsg {
        connected,
        signal_strength_percent: telemetry.signal_strength_percent,
        aircraft_battery_percent: telemetry.aircraft_battery_percent,
        last_update_unix_secs: telemetry.last_telemetry_unix_secs,
        detail,
    }
}

fn extract_crsf_frames(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut cursor = 0usize;
    while buf.len().saturating_sub(cursor) >= 3 {
        if buf[cursor] != CRSF_SYNC && buf[cursor] != 0xEA && buf[cursor] != 0xEE {
            cursor += 1;
            continue;
        }

        let packet_len = buf[cursor + 1] as usize + 2;
        if packet_len < 5 || packet_len > CRSF_MAX_PACKET_SIZE {
            cursor += 1;
            continue;
        }
        if cursor + packet_len > buf.len() {
            break;
        }

        let frame = &buf[cursor..cursor + packet_len];
        if check_frame_crc(frame) {
            frames.push(frame.to_vec());
            cursor += packet_len;
        } else {
            cursor += 1;
        }
    }

    if cursor > 0 {
        buf.drain(..cursor);
    }
    frames
}

fn check_frame_crc(frame: &[u8]) -> bool {
    if frame.len() < 5 {
        return false;
    }
    let len = frame[1] as usize;
    let expected = *frame.last().unwrap_or(&0);
    CRSF_CRC.checksum(&frame[2..2 + len - 1]) == expected
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::{
        build_elrs_state, check_frame_crc, derive_elrs_model_id, extract_crsf_frames,
        shift_power_level, wifi_exit_silence_active, EditorState, ElrsUiState, LinkMonitorState,
        TelemetryStatusState, CRSF_CRC, CRSF_FRAME_BATTERY_ID, CRSF_FRAME_LINK_ID, CRSF_SYNC,
    };
    use crate::{
        elrs::ElrsProtocolRuntime,
        messages::{UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget},
    };
    use std::time::{Duration, Instant};

    fn build_frame(frame_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.push(CRSF_SYNC);
        frame.push((payload.len() + 2) as u8);
        frame.push(frame_type);
        frame.extend_from_slice(payload);
        frame.push(CRSF_CRC.checksum(&frame[2..]));
        frame
    }

    #[test]
    fn test_extract_crsf_frames_with_noise() {
        let frame = build_frame(CRSF_FRAME_LINK_ID, &[1, 2, 80, 4, 5, 6, 7, 8, 9]);
        let mut buf = vec![0x00, 0x11, 0x22];
        buf.extend_from_slice(&frame);
        let frames = extract_crsf_frames(&mut buf);
        assert_eq!(frames.len(), 1);
        assert!(check_frame_crc(&frames[0]));
        assert!(buf.is_empty());
    }

    #[test]
    fn test_status_update_from_link_and_battery_frames() {
        let mut state = TelemetryStatusState::default();
        let link = build_frame(CRSF_FRAME_LINK_ID, &[0, 0, 75, 0, 0, 0, 0, 0, 0]);
        let batt = build_frame(CRSF_FRAME_BATTERY_ID, &[0, 0, 0, 0, 0, 0, 0, 44]);

        let status1 = state.consume_frame(&link).expect("link status");
        assert_eq!(status1.signal_strength_percent, 75);
        assert_eq!(status1.aircraft_battery_percent, 100);

        let status2 = state.consume_frame(&batt).expect("battery status");
        assert_eq!(status2.signal_strength_percent, 75);
        assert_eq!(status2.aircraft_battery_percent, 44);
    }

    #[test]
    fn test_shift_power_level_clamped() {
        assert_eq!(shift_power_level(100, -1), 25);
        assert_eq!(shift_power_level(100, 1), 250);
        assert_eq!(shift_power_level(10, -1), 10);
        assert_eq!(shift_power_level(1000, 1), 1000);
        assert_eq!(shift_power_level(50, 1), 100);
    }

    #[test]
    fn test_editor_state_cycle_and_cursor() {
        let mut editor = EditorState::new("ab");
        editor.move_cursor(1);
        editor.cycle_char(1);
        assert_eq!(editor.cursor, 1);
        assert_eq!(editor.as_string(), "ac");
        editor.move_cursor(-5);
        assert_eq!(editor.cursor, 0);
    }

    #[test]
    fn test_wifi_exit_silence_active_before_deadline() {
        let now = Instant::now();
        assert!(wifi_exit_silence_active(
            Some(now + Duration::from_millis(500)),
            now
        ));
    }

    #[test]
    fn test_wifi_exit_silence_inactive_after_deadline() {
        let now = Instant::now();
        assert!(!wifi_exit_silence_active(
            Some(now + Duration::from_millis(500)),
            now + Duration::from_secs(1)
        ));
        assert!(!wifi_exit_silence_active(None, now));
    }

    #[test]
    fn test_link_monitor_marks_bind_verified_on_link() {
        let mut monitor = LinkMonitorState::default();
        let now = Instant::now();
        monitor.on_bind_requested(now);
        assert_eq!(
            monitor.on_link_active().as_deref(),
            Some("Bind verified by telemetry link")
        );
    }

    #[test]
    fn test_build_elrs_state_defaults_to_rf_off() {
        let ui_state = ElrsUiState::load();
        let telemetry = TelemetryStatusState::default();
        let link_monitor = LinkMonitorState::default();
        let state = build_elrs_state(
            false,
            "RF output disabled".to_string(),
            &telemetry,
            &ui_state,
            &ElrsProtocolRuntime::default(),
            &link_monitor,
            "/dev/ttyS2",
        );

        assert!(!state.rf_output_enabled);
        assert!(!state.link_active);
        assert_eq!(state.params[0].id, "rf_output");
        assert_eq!(state.params[0].value, "OFF");
        assert_eq!(state.params[6].id, "link_state");
        assert_eq!(state.params[6].value, "RF OFF");
    }

    #[test]
    fn test_build_elrs_state_tx_power_prefers_runtime_current_power_value() {
        let mut ui_state = ElrsUiState::load();
        ui_state.config.rf_output_enabled = true;
        ui_state.config.tx_power_mw = 500;
        ui_state.config.tx_max_power_mw = 1000;

        let telemetry = TelemetryStatusState::default();
        let link_monitor = LinkMonitorState::default();
        let mut protocol = ElrsProtocolRuntime::default();

        let max_power_frame = vec![
            0xC8, 0x18, 0x2B, 0xEA, 0xEE, 0x01, 0x00, 0x00, 0x09, b'M', b'a', b'x', b' ', b'P',
            b'o', b'w', b'e', b'r', 0, b'2', b'5', b';', b'5', b'0', b'0', 0, 0x01, 0x00,
        ];
        let current_power_frame = vec![
            0xC8, 0x14, 0x2B, 0xEA, 0xEE, 0x02, 0x00, 0x00, 0x09, b'P', b'o', b'w', b'e', b'r', 0,
            b'2', b'5', b';', b'2', b'5', b'0', 0, 0x01, 0x00,
        ];
        protocol.consume_frame(&max_power_frame);
        protocol.consume_frame(&current_power_frame);

        let state = build_elrs_state(
            true,
            "ok".to_string(),
            &telemetry,
            &ui_state,
            &protocol,
            &link_monitor,
            "/dev/ttyS2",
        );

        assert_eq!(state.tx_power, "250");
        let max_entry = state
            .params
            .iter()
            .find(|entry| entry.id == "tx_max_power")
            .expect("tx_max_power entry");
        assert_eq!(max_entry.value, "1000mW");
    }

    #[test]
    fn test_tx_power_rejects_value_above_max_with_error_feedback() {
        let mut ui_state = ElrsUiState::load();
        ui_state.selected_idx = 3;
        ui_state.config.rf_output_enabled = true;
        ui_state.config.tx_power_mw = 10;
        ui_state.config.tx_max_power_mw = 10;

        let mut protocol = ElrsProtocolRuntime::default();
        let current_power_frame = vec![
            0xC8, 0x14, 0x2B, 0xEA, 0xEE, 0x02, 0x00, 0x00, 0x09, b'P', b'o', b'w', b'e', b'r', 0,
            b'1', b'0', b';', b'2', b'5', 0, 0x00, 0x00,
        ];
        protocol.consume_frame(&current_power_frame);

        let message = ui_state.adjust_selected(1, &mut protocol, true);
        assert_eq!(message, "TX power exceeds max 10mW");
        assert_eq!(ui_state.config.tx_power_mw, 10);

        let feedback = ui_state
            .interaction_feedback
            .as_ref()
            .expect("feedback should exist");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Error);
        assert_eq!(
            feedback.target,
            UiFeedbackTarget::FieldId("tx_power".to_string())
        );
    }

    #[test]
    fn test_tx_max_power_adjust_clamps_tx_power() {
        let mut ui_state = ElrsUiState::load();
        ui_state.selected_idx = 4;
        ui_state.config.rf_output_enabled = true;
        ui_state.config.tx_power_mw = 500;
        ui_state.config.tx_max_power_mw = 500;

        let mut protocol = ElrsProtocolRuntime::default();
        let max_power_frame = vec![
            0xC8, 0x18, 0x2B, 0xEA, 0xEE, 0x01, 0x00, 0x00, 0x09, b'M', b'a', b'x', b' ', b'P',
            b'o', b'w', b'e', b'r', 0, b'2', b'5', b';', b'5', b'0', b'0', 0, 0x01, 0x00,
        ];
        protocol.consume_frame(&max_power_frame);

        let message = ui_state.adjust_selected(-1, &mut protocol, true);
        assert_eq!(message, "TX max power request queued");
        assert_eq!(ui_state.config.tx_max_power_mw, 500);
        assert_eq!(ui_state.config.tx_power_mw, 500);

        let feedback = ui_state
            .interaction_feedback
            .as_ref()
            .expect("feedback should exist");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Busy);
        assert_eq!(
            feedback.target,
            UiFeedbackTarget::FieldId("tx_max_power".to_string())
        );
    }

    #[test]
    fn test_tx_power_requires_rf_output_enabled() {
        let mut ui_state = ElrsUiState::load();
        ui_state.selected_idx = 3;
        ui_state.config.rf_output_enabled = false;
        ui_state.config.tx_power_mw = 100;

        let message = ui_state.adjust_selected(1, &mut ElrsProtocolRuntime::default(), false);
        assert_eq!(message, "TX power unavailable: enable RF output first");
        assert_eq!(ui_state.config.tx_power_mw, 100);
        let feedback = ui_state
            .interaction_feedback
            .as_ref()
            .expect("feedback should exist");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Error);
    }

    #[test]
    fn test_tx_max_power_requires_rf_output_enabled() {
        let mut ui_state = ElrsUiState::load();
        ui_state.selected_idx = 4;
        ui_state.config.rf_output_enabled = false;
        ui_state.config.tx_max_power_mw = 500;

        let message = ui_state.adjust_selected(-1, &mut ElrsProtocolRuntime::default(), false);
        assert_eq!(message, "TX max power unavailable: enable RF output first");
        assert_eq!(ui_state.config.tx_max_power_mw, 500);
        let feedback = ui_state
            .interaction_feedback
            .as_ref()
            .expect("feedback should exist");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Error);
    }

    #[test]
    fn test_tx_power_queued_does_not_change_local_value_until_confirmation() {
        let mut ui_state = ElrsUiState::load();
        ui_state.selected_idx = 3;
        ui_state.config.rf_output_enabled = true;
        ui_state.config.tx_power_mw = 10;
        ui_state.config.tx_max_power_mw = 1000;
        let mut protocol = ElrsProtocolRuntime::default();

        let first_chunk = vec![
            0xC8, 0x11, 0x2B, 0xEA, 0xEE, 0x05, 0x01, 0x00, 0x09, b'T', b'X', b' ', b'P', b'o',
            b'w', b'e', b'r', 0x00,
        ];
        protocol.consume_frame(&first_chunk);

        let message = ui_state.adjust_selected(1, &mut protocol, true);
        assert_eq!(message, "TX power request queued");
        assert_eq!(ui_state.config.tx_power_mw, 10);
    }

    #[test]
    fn test_tx_power_is_read_only_when_only_max_power_exists() {
        let mut ui_state = ElrsUiState::load();
        ui_state.selected_idx = 3;
        ui_state.config.rf_output_enabled = true;
        ui_state.config.tx_power_mw = 10;
        ui_state.config.tx_max_power_mw = 25;
        let mut protocol = ElrsProtocolRuntime::default();

        let max_power_frame = vec![
            0xC8, 0x17, 0x2B, 0xEA, 0xEE, 0x06, 0x00, 0x00, 0x09, b'M', b'a', b'x', b' ', b'P',
            b'o', b'w', b'e', b'r', 0, b'1', b'0', b';', b'2', b'5', 0, 0x00, 0x00,
        ];
        protocol.consume_frame(&max_power_frame);

        let message = ui_state.adjust_selected(1, &mut protocol, true);
        assert_eq!(
            message,
            "TX power is read-only on this module (adjust TX max power)"
        );
        assert_eq!(ui_state.config.tx_power_mw, 10);
    }

    #[test]
    fn test_derive_elrs_model_id_is_stable() {
        assert_eq!(
            derive_elrs_model_id("quad_x"),
            derive_elrs_model_id("quad_x")
        );
        assert_ne!(
            derive_elrs_model_id("quad_x"),
            derive_elrs_model_id("fixed_wing")
        );
    }

    #[test]
    fn test_wifi_unavailable_emits_error_feedback_for_wifi_field() {
        let mut ui_state = ElrsUiState::load();
        ui_state.config.rf_output_enabled = false;
        ui_state.config.wifi_manual_on = false;
        ui_state.selected_idx = 1;

        let message = ui_state.adjust_selected(1, &mut ElrsProtocolRuntime::default(), false);

        assert_eq!(message, "WiFi unavailable: enable RF output first");
        let feedback = ui_state
            .interaction_feedback
            .as_ref()
            .expect("feedback should exist");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Error);
        assert_eq!(feedback.slot, UiFeedbackSlot::TopStatusBar);
        assert_eq!(
            feedback.target,
            UiFeedbackTarget::FieldId("wifi_manual".to_string())
        );
    }

    #[test]
    fn test_rf_output_toggle_emits_success_feedback_for_top_status_bar() {
        let mut ui_state = ElrsUiState::load();
        ui_state.selected_idx = 0;

        let _ = ui_state.adjust_selected(1, &mut ElrsProtocolRuntime::default(), false);

        let feedback = ui_state
            .interaction_feedback
            .as_ref()
            .expect("feedback should exist");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Success);
        assert_eq!(feedback.slot, UiFeedbackSlot::TopStatusBar);
        assert_eq!(
            feedback.target,
            UiFeedbackTarget::FieldId("rf_output".to_string())
        );
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("rf_link_service", rf_link_service_main);
    rpos::module::Module::register("elrs_tx", rf_link_service_main);
}
