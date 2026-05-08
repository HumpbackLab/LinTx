use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crc::{Crc, CRC_8_DVB_S2};

pub const CRSF_SYNC: u8 = 0xC8;
pub const MODULE_ADDRESS: u8 = 0xEE;
pub const RADIO_ADDRESS: u8 = 0xEA;
pub const ELRS_HANDSET_ADDRESS: u8 = 0xEF;
pub const COMMAND_ID: u8 = 0x32;
pub const SUBCOMMAND_CRSF: u8 = 0x10;
pub const SUBCOMMAND_CRSF_BIND: u8 = 0x01;
pub const COMMAND_MODEL_SELECT_ID: u8 = 0x05;

const CRSF_D5_CRC: Crc<u8> = Crc::<u8>::new(&CRC_8_DVB_S2);
const BIND_FRAME_INTERVAL: Duration = Duration::from_millis(120);
const BIND_SETTLE_INTERVAL: Duration = Duration::from_millis(400);
const PARAM_FRAME_INTERVAL: Duration = Duration::from_millis(80);
const WIFI_COMMAND_RETRY_INTERVAL: Duration = Duration::from_millis(180);
const TX_POWER_CONFIRM_TIMEOUT: Duration = Duration::from_millis(5000);
const TX_POWER_CONFIRM_QUERY_INTERVAL: Duration = Duration::from_millis(700);
const PARAM_MAX_FIELD_ID: u8 = 64;
const PARAM_DEVICE_PING_ID: u8 = 0x28;
const PARAM_DEVICE_INFO_ID: u8 = 0x29;
const PARAM_REQUEST_SETTINGS_ID: u8 = 0x2A;
const PARAM_SETTINGS_ENTRY_ID: u8 = 0x2B;
const PARAM_READ_ID: u8 = 0x2C;
const PARAM_WRITE_ID: u8 = 0x2D;
const CRSF_LINK_ID: u8 = 0x14;
// 0x2E 是 ELRS 私有链路统计帧（elrsV3.lua parseElrsInfoMessage），不是命令状态响应。
// 命令执行的响应通过 0x2B（PARAM_SETTINGS_ENTRY_ID）返回，字段 status 字节反映执行状态。
const ELRS_LINK_STAT_ID: u8 = 0x2E;
const TX_POWER_CURRENT_LABELS: &[&str] = &["Power", "TX Power", "RF Power", "Output Power"];
const TX_POWER_MAX_LABELS: &[&str] = &["Max Power"];
const TX_POWER_FALLBACK_LABELS: &[&str] =
    &["power", "tx power", "rf power", "output power", "max power"];
const TX_POWER_CURRENT_FALLBACK_LABELS: &[&str] =
    &["power", "tx power", "rf power", "output power"];
const TX_POWER_FALLBACK_LEVELS_MW: &[u16] = &[10, 25, 50, 100, 250, 500, 1000, 2000];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElrsOperation {
    EnterBind,
    SetWifiManual(bool),
    SetTxPower(u16),
    SetTxMaxPower(u16),
    SetBindPhrase(String),
    RefreshParams,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElrsOperationStatus {
    Queued(&'static str),
    Busy(&'static str),
    Unsupported(&'static str),
}

impl ElrsOperationStatus {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Queued(msg) | Self::Busy(msg) | Self::Unsupported(msg) => msg,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    pub serial: u32,
    pub hw_version: u32,
    pub sw_version: u32,
    pub field_count: u8,
    pub is_elrs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Folder,
    Select,
    Command,
    Info,
    String,
    Unknown(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStep {
    Ready,
    Start,
    Progress,
    ConfirmationNeeded,
    Confirm,
    Cancel,
    Poll,
    Unknown(u8),
}

impl CommandStep {
    fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::Ready,
            1 => Self::Start,
            2 => Self::Progress,
            3 => Self::ConfirmationNeeded,
            4 => Self::Confirm,
            5 => Self::Cancel,
            6 => Self::Poll,
            other => Self::Unknown(other),
        }
    }

    fn raw(&self) -> u8 {
        match self {
            Self::Ready => 0,
            Self::Start => 1,
            Self::Progress => 2,
            Self::ConfirmationNeeded => 3,
            Self::Confirm => 4,
            Self::Cancel => 5,
            Self::Poll => 6,
            Self::Unknown(raw) => *raw,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Start => "START",
            Self::Progress => "PROGRESS",
            Self::ConfirmationNeeded => "CONFIRMATION_NEEDED",
            Self::Confirm => "CONFIRM",
            Self::Cancel => "CANCEL",
            Self::Poll => "POLL",
            Self::Unknown(_) => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandStatus {
    pub step: CommandStep,
    pub timeout: u8,
    pub info: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamEntry {
    pub id: u8,
    pub parent: u8,
    pub kind: ParamKind,
    pub label: String,
    pub options: Vec<String>,
    pub value: String,
    pub command_status: Option<CommandStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingCommand {
    field_id: u8,
    action: PendingCommandAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingCommandAction {
    WifiEnable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingParamChunk {
    parent: u8,
    kind_raw: u8,
    data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingTxPowerWrite {
    field_id: u8,
    expected_index: u8,
    expected_mw: Option<u16>,
    kind: PendingTxPowerKind,
    requested_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingTxPowerKind {
    Current,
    Max,
}

#[derive(Debug, Clone)]
pub struct ElrsProtocolRuntime {
    bind_frames_remaining: u8,
    bind_frames_total: u8,
    bind_command_pending: bool,
    last_bind_frame_at: Option<Instant>,
    bind_settle_until: Option<Instant>,
    last_param_frame_at: Option<Instant>,
    last_ping_at: Option<Instant>,
    last_status: Option<String>,
    param_refresh_requested: bool,
    model_id_pending: bool,
    settings_request_pending: bool,
    current_model_id: u8,
    next_param_read_id: u8,
    outgoing_queue: VecDeque<Vec<u8>>,
    device_info: Option<DeviceInfo>,
    params: HashMap<u8, ParamEntry>,
    pending_command: Option<PendingCommand>,
    pending_param_chunks: HashMap<u8, PendingParamChunk>,
    pending_tx_power_write: Option<PendingTxPowerWrite>,
    last_tx_power_query_at: Option<Instant>,
    last_wifi_command_at: Option<Instant>,
}

impl Default for ElrsProtocolRuntime {
    fn default() -> Self {
        Self {
            bind_frames_remaining: 0,
            bind_frames_total: 0,
            bind_command_pending: false,
            last_bind_frame_at: None,
            bind_settle_until: None,
            last_param_frame_at: None,
            last_ping_at: None,
            last_status: None,
            param_refresh_requested: true,
            model_id_pending: true,
            settings_request_pending: true,
            current_model_id: 0,
            next_param_read_id: 1,
            outgoing_queue: VecDeque::new(),
            device_info: None,
            params: HashMap::new(),
            pending_command: None,
            pending_param_chunks: HashMap::new(),
            pending_tx_power_write: None,
            last_tx_power_query_at: None,
            last_wifi_command_at: None,
        }
    }
}

impl ElrsProtocolRuntime {
    pub fn request(&mut self, op: ElrsOperation) -> ElrsOperationStatus {
        match op {
            ElrsOperation::EnterBind => {
                if self.bind_active() {
                    ElrsOperationStatus::Busy("Bind already in progress")
                } else if let Some(id) = self.find_command_param(&["Bind"]) {
                    self.outgoing_queue.push_back(build_parameter_command_frame(
                        MODULE_ADDRESS,
                        id,
                        1,
                    ));
                    self.bind_frames_remaining = 1;
                    self.bind_frames_total = 1;
                    self.bind_command_pending = true;
                    self.last_bind_frame_at = None;
                    self.bind_settle_until = None;
                    self.last_status = Some("Bind command queued".to_string());
                    ElrsOperationStatus::Queued("Bind command queued")
                } else {
                    self.request_refresh();
                    ElrsOperationStatus::Unsupported("Bind command unavailable")
                }
            }
            ElrsOperation::SetWifiManual(enable) => {
                if !enable {
                    // ELRS 3.x 没有通过 CRSF 关闭 WiFi 的命令。
                    // 模块进入 WiFi 后必须断电重启才能恢复，无需发送帧。
                    // 返回 Queued 让调用方清除本地 wifi_manual_on 标志。
                    self.drop_pending_wifi_frames();
                    self.pending_command = None;
                    self.last_wifi_command_at = None;
                    self.last_status = Some("WiFi state cleared".to_string());
                    return ElrsOperationStatus::Queued("WiFi state cleared");
                }
                let labels: &[&str] = &["Enable WiFi", "Enter WiFi", "WiFi Update"];
                // 精确标签匹配；若失败则回退到标签中包含 "wifi" 的任意 COMMAND 字段
                let wifi_id = self
                    .find_command_param(labels)
                    .or_else(|| self.find_wifi_command_fallback())
                    .or_else(|| self.find_pending_wifi_command(labels));
                if let Some(id) = wifi_id {
                    self.queue_command_step(id, CommandStep::Start);
                    self.pending_command = Some(PendingCommand {
                        field_id: id,
                        action: PendingCommandAction::WifiEnable,
                    });
                    self.last_status = Some("WiFi command queued".to_string());
                    ElrsOperationStatus::Queued("WiFi command queued")
                } else if self.device_info.is_some() {
                    // 模块已被识别，参数还在枚举中——不要打断正在进行的枚举。
                    // 返回 Busy 让调用方提示用户等待，而不是清空已枚举到的数据重来。
                    ElrsOperationStatus::Busy("WiFi params loading, please wait")
                } else {
                    // 模块还未被识别，触发重新发现流程。
                    self.request_refresh();
                    ElrsOperationStatus::Unsupported("WiFi unavailable: module not found")
                }
            }
            ElrsOperation::SetTxPower(power) => {
                let (param_id, index, selected) =
                    if let Some((param_id, param_label, param_options)) = self
                        .find_current_tx_power_select_param()
                        .map(|param| (param.id, param.label.clone(), param.options.clone()))
                    {
                        let index = if let Some(index) = nearest_option_index(&param_options, power)
                        {
                            index
                        } else if param_options.is_empty()
                            && is_tx_power_label_candidate(&param_label)
                        {
                            fallback_tx_power_option_index(power)
                        } else {
                            self.request_refresh();
                            return ElrsOperationStatus::Unsupported(
                                "TX power options unavailable",
                            );
                        };
                        let selected = param_options
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("{}mW", TX_POWER_FALLBACK_LEVELS_MW[index]));
                        (param_id, index, selected)
                    } else if let Some((field_id, _label)) =
                        self.find_pending_current_tx_power_select_field()
                    {
                        let index = fallback_tx_power_option_index(power);
                        let selected = format!("{}mW", TX_POWER_FALLBACK_LEVELS_MW[index]);
                        (field_id, index, selected)
                    } else if power >= 100 {
                        if let Some((param_id, param_options)) = self
                            .find_max_tx_power_select_param()
                            .map(|param| (param.id, param.options.clone()))
                        {
                            let Some(index) = nearest_option_index(&param_options, power) else {
                                self.request_refresh();
                                return ElrsOperationStatus::Unsupported(
                                    "TX power options unavailable",
                                );
                            };
                            let selected = param_options[index].clone();
                            (param_id, index, selected)
                        } else {
                            self.request_refresh();
                            return ElrsOperationStatus::Unsupported(
                                "TX power is read-only on this module (adjust TX max power)",
                            );
                        }
                    } else {
                        self.request_refresh();
                        return ElrsOperationStatus::Unsupported(
                            "TX power is read-only on this module (adjust TX max power)",
                        );
                    };
                self.outgoing_queue.push_back(build_parameter_write_frame(
                    MODULE_ADDRESS,
                    param_id,
                    &[index as u8],
                ));
                self.pending_tx_power_write = Some(PendingTxPowerWrite {
                    field_id: param_id,
                    expected_index: index as u8,
                    expected_mw: parse_power_value(&selected),
                    kind: PendingTxPowerKind::Current,
                    requested_at: Instant::now(),
                });
                self.last_tx_power_query_at = None;
                self.last_status = Some(format!("TX power request queued: {}", selected));
                ElrsOperationStatus::Queued("TX power request queued")
            }
            ElrsOperation::SetTxMaxPower(power) => {
                let Some((param_id, param_options)) = self
                    .find_max_tx_power_select_param()
                    .map(|param| (param.id, param.options.clone()))
                else {
                    self.request_refresh();
                    return ElrsOperationStatus::Unsupported("TX max power parameter unavailable");
                };
                let Some(index) = nearest_option_index(&param_options, power) else {
                    self.request_refresh();
                    return ElrsOperationStatus::Unsupported("TX max power options unavailable");
                };
                self.outgoing_queue.push_back(build_parameter_write_frame(
                    MODULE_ADDRESS,
                    param_id,
                    &[index as u8],
                ));
                self.pending_tx_power_write = Some(PendingTxPowerWrite {
                    field_id: param_id,
                    expected_index: index as u8,
                    expected_mw: parse_power_value(&param_options[index]),
                    kind: PendingTxPowerKind::Max,
                    requested_at: Instant::now(),
                });
                self.last_tx_power_query_at = None;
                self.last_status = Some(format!(
                    "TX max power request queued: {}",
                    param_options[index]
                ));
                ElrsOperationStatus::Queued("TX max power request queued")
            }
            ElrsOperation::SetBindPhrase(value) => {
                if let Some(id) = self.find_string_param(&["Bind Phrase"]) {
                    self.outgoing_queue.push_back(build_parameter_write_frame(
                        MODULE_ADDRESS,
                        id,
                        value.as_bytes(),
                    ));
                    self.last_status = Some("Bind phrase write queued".to_string());
                    ElrsOperationStatus::Queued("Bind phrase write queued")
                } else {
                    self.request_refresh();
                    ElrsOperationStatus::Unsupported("Bind phrase parameter unavailable")
                }
            }
            ElrsOperation::RefreshParams => {
                self.request_refresh();
                ElrsOperationStatus::Queued("ELRS params refresh queued")
            }
        }
    }

    pub fn bind_active(&self) -> bool {
        self.bind_frames_remaining > 0
            || self
                .bind_settle_until
                .map(|deadline| Instant::now() < deadline)
                .unwrap_or(false)
            || self.bind_command_pending
    }

    pub fn clear_ephemeral(&mut self) {
        self.bind_frames_remaining = 0;
        self.bind_frames_total = 0;
        self.bind_command_pending = false;
        self.last_bind_frame_at = None;
        self.bind_settle_until = None;
        self.outgoing_queue.clear();
        self.pending_command = None;
        self.pending_param_chunks.clear();
        self.pending_tx_power_write = None;
        self.last_tx_power_query_at = None;
        self.last_wifi_command_at = None;
    }

    pub fn request_refresh(&mut self) {
        self.param_refresh_requested = true;
        self.model_id_pending = true;
        self.settings_request_pending = true;
        self.last_ping_at = None;
    }

    pub fn set_model_id(&mut self, model_id: u8) {
        if self.current_model_id != model_id {
            self.current_model_id = model_id;
            self.model_id_pending = true;
            self.last_status = Some(format!("ELRS model id set to {}", model_id));
        }
    }

    pub fn poll_outgoing_frame(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self.bind_frames_remaining > 0 && !self.bind_command_pending {
            if let Some(last) = self.last_bind_frame_at {
                if now.saturating_duration_since(last) < BIND_FRAME_INTERVAL {
                    return None;
                }
            }
            self.last_bind_frame_at = Some(now);
            self.bind_frames_remaining = self.bind_frames_remaining.saturating_sub(1);
            if self.bind_frames_remaining == 0 {
                self.bind_settle_until = Some(now + BIND_SETTLE_INTERVAL);
                self.last_status = Some("Bind command burst sent".to_string());
            }
            return Some(build_crossfire_bind_frame(MODULE_ADDRESS));
        }

        if let Some(deadline) = self.bind_settle_until {
            if now < deadline {
                return None;
            }
            self.bind_settle_until = None;
        }

        if let Some(last) = self.last_param_frame_at {
            if now.saturating_duration_since(last) < PARAM_FRAME_INTERVAL {
                return None;
            }
        }

        if let Some(frame) = self.outgoing_queue.pop_front() {
            if self.bind_command_pending {
                self.bind_command_pending = false;
                self.bind_frames_remaining = self.bind_frames_remaining.saturating_sub(1);
                self.bind_settle_until = Some(now + BIND_SETTLE_INTERVAL);
                self.last_status = Some("Bind command sent".to_string());
            }
            if is_pending_wifi_start_frame(self.pending_command.as_ref(), &frame) {
                self.last_wifi_command_at = Some(now);
            }
            self.last_param_frame_at = Some(now);
            return Some(frame);
        }

        if let Some(pending) = self.pending_tx_power_write.clone() {
            if now.saturating_duration_since(pending.requested_at) >= TX_POWER_CONFIRM_TIMEOUT {
                self.pending_tx_power_write = None;
                self.last_tx_power_query_at = None;
                let timeout = match pending.kind {
                    PendingTxPowerKind::Current => "TX power confirmation timeout",
                    PendingTxPowerKind::Max => "TX max power confirmation timeout",
                };
                self.last_status = Some(timeout.to_string());
            }
        }

        if let Some(pending) = self.pending_tx_power_write.as_ref() {
            let query_due = self
                .last_tx_power_query_at
                .map(|last| now.saturating_duration_since(last) >= TX_POWER_CONFIRM_QUERY_INTERVAL)
                .unwrap_or(true);
            if query_due {
                self.last_tx_power_query_at = Some(now);
                self.last_param_frame_at = Some(now);
                return Some(build_parameter_read_frame(
                    MODULE_ADDRESS,
                    pending.field_id,
                    0,
                ));
            }
        }

        if let Some(PendingCommand {
            field_id,
            action: PendingCommandAction::WifiEnable,
        }) = self.pending_command.as_ref()
        {
            let retry_due = self
                .last_wifi_command_at
                .map(|last| now.saturating_duration_since(last) >= WIFI_COMMAND_RETRY_INTERVAL)
                .unwrap_or(true);
            if retry_due {
                self.last_wifi_command_at = Some(now);
                self.last_param_frame_at = Some(now);
                return Some(build_parameter_command_frame(
                    MODULE_ADDRESS,
                    *field_id,
                    CommandStep::Start.raw(),
                ));
            }
        }

        if self.model_id_pending {
            self.model_id_pending = false;
            self.last_param_frame_at = Some(now);
            return Some(build_crossfire_model_id_frame(
                MODULE_ADDRESS,
                self.current_model_id,
            ));
        }

        if self.settings_request_pending && self.device_info.is_some() {
            self.settings_request_pending = false;
            self.last_param_frame_at = Some(now);
            return Some(build_request_settings_frame(MODULE_ADDRESS));
        }

        let ping_due = self
            .last_ping_at
            .map(|last| now.saturating_duration_since(last) >= Duration::from_secs(2))
            .unwrap_or(true);
        if (self.param_refresh_requested || self.device_info.is_none()) && ping_due {
            self.last_ping_at = Some(now);
            self.last_param_frame_at = Some(now);
            if self.param_refresh_requested {
                self.param_refresh_requested = false;
                self.next_param_read_id = 1;
                self.params.clear();
                self.pending_param_chunks.clear();
            }
            return Some(build_device_ping_frame(MODULE_ADDRESS));
        }

        let scan_limit = self
            .device_info
            .as_ref()
            .map(|d| d.field_count)
            .unwrap_or(PARAM_MAX_FIELD_ID);
        if self.device_info.is_some() && self.next_param_read_id <= scan_limit {
            let frame = build_parameter_read_frame(MODULE_ADDRESS, self.next_param_read_id, 0);
            self.next_param_read_id = self.next_param_read_id.saturating_add(1);
            self.last_param_frame_at = Some(now);
            return Some(frame);
        }

        None
    }

    pub fn take_status_text(&mut self) -> Option<String> {
        self.last_status.take()
    }

    pub fn is_pending_wifi_start_frame(&self, frame: &[u8]) -> bool {
        is_pending_wifi_start_frame(self.pending_command.as_ref(), frame)
    }

    pub fn bind_progress(&self) -> Option<(u8, u8)> {
        if self.bind_frames_total == 0 {
            None
        } else {
            Some((
                self.bind_frames_total
                    .saturating_sub(self.bind_frames_remaining),
                self.bind_frames_total,
            ))
        }
    }

    pub fn consume_frame(&mut self, frame: &[u8]) {
        if frame.len() < 5 {
            return;
        }
        match frame[2] {
            PARAM_DEVICE_INFO_ID => {
                if let Some(info) = parse_device_info(frame) {
                    self.next_param_read_id = 1;
                    self.device_info = Some(info.clone());
                    self.last_status = Some(format!("ELRS module detected: {}", info.name));
                }
            }
            PARAM_SETTINGS_ENTRY_ID => {
                if let Some(entry) = self.consume_parameter_frame(frame) {
                    if let Some(status_text) = self
                        .update_pending_command(&entry)
                        .or_else(|| self.update_pending_tx_power(&entry))
                    {
                        self.last_status = Some(status_text);
                    }
                    self.params.insert(entry.id, entry);
                }
            }
            PARAM_WRITE_ID => {
                if let Some(status_text) = self.update_pending_tx_power_from_write_ack(frame) {
                    self.last_status = Some(status_text);
                }
            }
            CRSF_LINK_ID => {
                if let Some(status_text) = self.update_pending_tx_power_from_link(frame) {
                    self.last_status = Some(status_text);
                }
            }
            ELRS_LINK_STAT_ID => {
                // ELRS 私有链路统计帧，暂不解析，仅记录已收到
            }
            _ => {}
        }
    }

    pub fn device_info(&self) -> Option<&DeviceInfo> {
        self.device_info.as_ref()
    }

    pub fn select_tx_power_value(&self) -> Option<String> {
        self.find_current_tx_power_select_param()
            .map(|param| param.value.clone())
    }

    pub fn has_current_tx_power_control(&self) -> bool {
        self.find_current_tx_power_select_param().is_some()
            || self.find_pending_current_tx_power_select_field().is_some()
    }

    pub fn select_tx_max_power_value(&self) -> Option<String> {
        self.find_max_tx_power_select_param()
            .map(|param| param.value.clone())
    }

    fn find_command_param(&self, labels: &[&str]) -> Option<u8> {
        self.params
            .values()
            .find(|param| {
                matches!(param.kind, ParamKind::Command)
                    && labels.iter().any(|label| param.label == *label)
            })
            .map(|param| param.id)
    }

    fn find_pending_wifi_command(&self, labels: &[&str]) -> Option<u8> {
        self.pending_param_chunks
            .iter()
            .find_map(|(&field_id, pending)| {
                if !matches!(parse_param_kind(pending.kind_raw), ParamKind::Command) {
                    return None;
                }
                let label_end = pending
                    .data
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(pending.data.len());
                let label = String::from_utf8_lossy(&pending.data[..label_end]).to_string();
                if labels.iter().any(|expected| *expected == label) {
                    return Some(field_id);
                }
                let lower = label.to_ascii_lowercase();
                let matches_wifi_prefix = lower.contains("wifi")
                    || (lower.len() >= 6
                        && [
                            "enable wifi",
                            "enter wifi",
                            "wifi update",
                            "wifi connectivity",
                        ]
                        .iter()
                        .any(|full| full.starts_with(lower.as_str())));
                if matches_wifi_prefix {
                    Some(field_id)
                } else {
                    None
                }
            })
    }

    fn find_select_param(&self, labels: &[&str]) -> Option<&ParamEntry> {
        for label in labels {
            if let Some(param) = self
                .params
                .values()
                .filter(|param| matches!(param.kind, ParamKind::Select) && param.label == *label)
                .min_by_key(|param| param.id)
            {
                return Some(param);
            }
        }
        None
    }

    fn find_tx_power_select_param(&self) -> Option<&ParamEntry> {
        self.find_current_tx_power_select_param()
            .or_else(|| self.find_max_tx_power_select_param())
            .or_else(|| self.find_tx_power_select_fallback())
    }

    fn find_current_tx_power_select_param(&self) -> Option<&ParamEntry> {
        self.find_select_param(TX_POWER_CURRENT_LABELS)
            .or_else(|| self.find_current_tx_power_select_fallback())
    }

    fn find_max_tx_power_select_param(&self) -> Option<&ParamEntry> {
        self.find_select_param(TX_POWER_MAX_LABELS)
    }

    fn find_tx_power_select_fallback(&self) -> Option<&ParamEntry> {
        self.params
            .values()
            .filter(|param| matches!(param.kind, ParamKind::Select))
            .filter_map(|param| {
                tx_power_label_priority(&param.label).map(|priority| (priority, param.id, param))
            })
            .min_by_key(|(priority, id, _)| (*priority, *id))
            .map(|(_, _, param)| param)
    }

    fn find_current_tx_power_select_fallback(&self) -> Option<&ParamEntry> {
        self.params
            .values()
            .filter(|param| matches!(param.kind, ParamKind::Select))
            .filter_map(|param| {
                current_tx_power_label_priority(&param.label)
                    .map(|priority| (priority, param.id, param))
            })
            .min_by_key(|(priority, id, _)| (*priority, *id))
            .map(|(_, _, param)| param)
    }

    fn find_pending_tx_power_select_field(&self) -> Option<(u8, String)> {
        self.pending_param_chunks
            .iter()
            .filter_map(|(&field_id, pending)| {
                if !matches!(parse_param_kind(pending.kind_raw), ParamKind::Select) {
                    return None;
                }
                let label_end = pending
                    .data
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(pending.data.len());
                let label = String::from_utf8_lossy(&pending.data[..label_end]).to_string();
                tx_power_label_priority(&label).map(|priority| (priority, field_id, label))
            })
            .min_by_key(|(priority, field_id, _)| (*priority, *field_id))
            .map(|(_, field_id, label)| (field_id, label))
    }

    fn find_pending_current_tx_power_select_field(&self) -> Option<(u8, String)> {
        self.pending_param_chunks
            .iter()
            .filter_map(|(&field_id, pending)| {
                if !matches!(parse_param_kind(pending.kind_raw), ParamKind::Select) {
                    return None;
                }
                let label_end = pending
                    .data
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(pending.data.len());
                let label = String::from_utf8_lossy(&pending.data[..label_end]).to_string();
                current_tx_power_label_priority(&label).map(|priority| (priority, field_id, label))
            })
            .min_by_key(|(priority, field_id, _)| (*priority, *field_id))
            .map(|(_, field_id, label)| (field_id, label))
    }

    fn find_string_param(&self, labels: &[&str]) -> Option<u8> {
        self.params
            .values()
            .find(|param| {
                matches!(param.kind, ParamKind::String)
                    && labels.iter().any(|label| param.label == *label)
            })
            .map(|param| param.id)
    }

    fn find_wifi_command_fallback(&self) -> Option<u8> {
        // 已知 WiFi 命令标签的完整小写形式，用于前缀匹配截断标签。
        // ELRS 模块使用 8 字节数据块，"Enable WiFi" 会被切成 "Enable W" + "iFi\0..."
        // 两个 chunk。第一个 chunk 没有 null 终止符，label 以截断形式入库。
        const WIFI_FULL_LABELS: &[&str] = &[
            "enable wifi",
            "enter wifi",
            "wifi update",
            "wifi connectivity",
        ];
        self.params
            .values()
            .find(|param| {
                if !matches!(param.kind, ParamKind::Command) {
                    return false;
                }
                let lower = param.label.to_ascii_lowercase();
                // 完整 label 包含 "wifi"（label 完整时）
                if lower.contains("wifi") {
                    return true;
                }
                // 前缀匹配：某已知完整 label 以截断 label 开头
                // 例："enable wifi".starts_with("enable w") == true
                // 至少 6 个字符才做前缀匹配，避免过短标签误匹配
                lower.len() >= 6
                    && WIFI_FULL_LABELS
                        .iter()
                        .any(|full| full.starts_with(lower.as_str()))
            })
            .map(|param| param.id)
    }

    fn queue_command_step(&mut self, field_id: u8, step: CommandStep) {
        self.outgoing_queue.push_back(build_parameter_command_frame(
            MODULE_ADDRESS,
            field_id,
            step.raw(),
        ));
    }

    fn drop_pending_wifi_frames(&mut self) {
        let pending = self.pending_command.clone();
        self.outgoing_queue
            .retain(|frame| !is_pending_wifi_start_frame(pending.as_ref(), frame));
    }

    fn update_pending_command(&mut self, entry: &ParamEntry) -> Option<String> {
        let Some(pending) = self.pending_command.clone() else {
            return None;
        };
        if pending.field_id != entry.id {
            return None;
        }
        let Some(status) = entry.command_status.as_ref() else {
            return None;
        };
        match (&pending.action, &status.step) {
            (PendingCommandAction::WifiEnable, CommandStep::Ready) => {
                self.pending_command = None;
                self.last_wifi_command_at = None;
                Some(format!("WiFi command ready{}", suffix_info(&status.info)))
            }
            (PendingCommandAction::WifiEnable, CommandStep::Progress) => Some(format!(
                "WiFi command in progress{}",
                suffix_info(&status.info)
            )),
            (PendingCommandAction::WifiEnable, CommandStep::ConfirmationNeeded) => {
                self.queue_command_step(entry.id, CommandStep::Confirm);
                Some(format!(
                    "WiFi command confirm queued{}",
                    suffix_info(&status.info)
                ))
            }
            (PendingCommandAction::WifiEnable, CommandStep::Poll) => {
                self.queue_command_step(entry.id, CommandStep::Poll);
                Some(format!(
                    "WiFi command poll queued{}",
                    suffix_info(&status.info)
                ))
            }
            (PendingCommandAction::WifiEnable, CommandStep::Cancel) => {
                self.pending_command = None;
                self.last_wifi_command_at = None;
                Some(format!(
                    "WiFi command canceled{}",
                    suffix_info(&status.info)
                ))
            }
            (PendingCommandAction::WifiEnable, CommandStep::Start) => {
                Some(format!("WiFi command started{}", suffix_info(&status.info)))
            }
            (PendingCommandAction::WifiEnable, CommandStep::Confirm) => Some(format!(
                "WiFi command confirming{}",
                suffix_info(&status.info)
            )),
            (PendingCommandAction::WifiEnable, CommandStep::Unknown(raw)) => Some(format!(
                "WiFi command status 0x{raw:02X}{}",
                suffix_info(&status.info)
            )),
        }
    }

    fn update_pending_tx_power(&mut self, entry: &ParamEntry) -> Option<String> {
        let Some(pending) = self.pending_tx_power_write.clone() else {
            return None;
        };
        let select_confirmed = pending.field_id == entry.id
            && matches!(entry.kind, ParamKind::Select)
            && entry
                .options
                .get(pending.expected_index as usize)
                .map(|expected| entry.value == *expected)
                .unwrap_or(false);

        let telemetry_confirmed = matches!(pending.kind, PendingTxPowerKind::Current)
            && entry.id == ELRS_LINK_STAT_ID
            && pending
                .expected_mw
                .map(|expected| parse_power_value(&entry.value) == Some(expected))
                .unwrap_or(false);

        if !select_confirmed && !telemetry_confirmed {
            return None;
        }
        self.pending_tx_power_write = None;
        self.last_tx_power_query_at = None;
        let expected_text = if telemetry_confirmed {
            pending
                .expected_mw
                .map(|value| format!("{}mW", value))
                .unwrap_or_else(|| entry.value.clone())
        } else {
            entry
                .options
                .get(pending.expected_index as usize)
                .cloned()
                .unwrap_or_else(|| entry.value.clone())
        };
        let message = match pending.kind {
            PendingTxPowerKind::Current => format!("TX power applied: {}", expected_text),
            PendingTxPowerKind::Max => format!("TX max power applied: {}", expected_text),
        };
        Some(message)
    }

    fn update_pending_tx_power_from_link(&mut self, frame: &[u8]) -> Option<String> {
        let pending = self.pending_tx_power_write.clone()?;
        if !matches!(pending.kind, PendingTxPowerKind::Current) {
            return None;
        }
        let expected = pending.expected_mw?;
        let actual = parse_link_frame_tx_power_mw(frame)?;
        if actual != expected {
            return None;
        }
        self.pending_tx_power_write = None;
        self.last_tx_power_query_at = None;
        Some(format!("TX power applied: {}mW", actual))
    }

    fn update_pending_tx_power_from_write_ack(&mut self, frame: &[u8]) -> Option<String> {
        let pending = self.pending_tx_power_write.clone()?;
        if frame.len() < 8 || frame.get(2).copied()? != PARAM_WRITE_ID {
            return None;
        }
        let field_id = frame.get(5).copied()?;
        let value = frame.get(6).copied()?;
        if field_id != pending.field_id || value != pending.expected_index {
            return None;
        }
        self.pending_tx_power_write = None;
        self.last_tx_power_query_at = None;
        let message = match pending.kind {
            PendingTxPowerKind::Current => format!("TX power applied: idx {}", value),
            PendingTxPowerKind::Max => format!("TX max power applied: idx {}", value),
        };
        Some(message)
    }

    fn consume_parameter_frame(&mut self, frame: &[u8]) -> Option<ParamEntry> {
        if frame.len() < 11 || frame[2] != PARAM_SETTINGS_ENTRY_ID {
            return None;
        }
        let field_id = *frame.get(5)?;
        let chunks_remain = *frame.get(6)?;
        let parent = *frame.get(7)?;
        let kind_raw = *frame.get(8)?;
        let data = frame.get(9..frame.len().saturating_sub(1))?;

        if chunks_remain > 0 {
            let slot = self
                .pending_param_chunks
                .entry(field_id)
                .or_insert_with(|| PendingParamChunk {
                    parent,
                    kind_raw,
                    data: Vec::new(),
                });
            if slot.parent != parent || slot.kind_raw != kind_raw {
                slot.parent = parent;
                slot.kind_raw = kind_raw;
                slot.data.clear();
            }
            slot.data.extend_from_slice(data);
            return None;
        }

        if let Some(mut pending) = self.pending_param_chunks.remove(&field_id) {
            if pending.parent == parent && pending.kind_raw == kind_raw {
                pending.data.extend_from_slice(data);
                let assembled =
                    build_parameter_entry_frame(field_id, parent, kind_raw, &pending.data);
                parse_parameter_entry(&assembled)
            } else {
                parse_parameter_entry(frame)
            }
        } else {
            parse_parameter_entry(frame)
        }
    }
}

pub fn build_crossfire_bind_frame(destination: u8) -> Vec<u8> {
    let mut frame = Vec::with_capacity(9);
    frame.push(CRSF_SYNC);
    frame.push(7);
    frame.push(COMMAND_ID);
    frame.push(destination);
    frame.push(RADIO_ADDRESS);
    frame.push(SUBCOMMAND_CRSF);
    frame.push(SUBCOMMAND_CRSF_BIND);
    frame.push(crc8_ba(&frame[2..7]));
    frame.push(CRSF_D5_CRC.checksum(&frame[2..8]));
    frame
}

pub fn build_crossfire_model_id_frame(destination: u8, model_id: u8) -> Vec<u8> {
    let mut frame = Vec::with_capacity(10);
    frame.push(CRSF_SYNC);
    frame.push(8);
    frame.push(COMMAND_ID);
    frame.push(destination);
    frame.push(RADIO_ADDRESS);
    frame.push(SUBCOMMAND_CRSF);
    frame.push(COMMAND_MODEL_SELECT_ID);
    frame.push(model_id);
    frame.push(crc8_ba(&frame[2..8]));
    frame.push(CRSF_D5_CRC.checksum(&frame[2..9]));
    frame
}

fn build_device_ping_frame(destination: u8) -> Vec<u8> {
    let _ = destination;
    let mut frame = Vec::with_capacity(6);
    frame.push(CRSF_SYNC);
    frame.push(4);
    frame.push(PARAM_DEVICE_PING_ID);
    frame.push(0x00);
    frame.push(RADIO_ADDRESS);
    frame.push(CRSF_D5_CRC.checksum(&frame[2..]));
    frame
}

fn build_request_settings_frame(destination: u8) -> Vec<u8> {
    build_extended_frame(
        PARAM_REQUEST_SETTINGS_ID,
        &[destination, parameter_handset_address(destination)],
    )
}

fn build_parameter_read_frame(destination: u8, field_id: u8, chunk: u8) -> Vec<u8> {
    build_extended_frame(
        PARAM_READ_ID,
        &[
            destination,
            parameter_handset_address(destination),
            field_id,
            chunk,
        ],
    )
}

fn build_parameter_write_frame(destination: u8, field_id: u8, value: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(4 + value.len());
    payload.extend_from_slice(&[
        destination,
        parameter_handset_address(destination),
        field_id,
    ]);
    payload.extend_from_slice(value);
    build_extended_frame(PARAM_WRITE_ID, &payload)
}

fn build_parameter_command_frame(destination: u8, field_id: u8, status: u8) -> Vec<u8> {
    build_parameter_write_frame(destination, field_id, &[status])
}

fn build_parameter_entry_frame(field_id: u8, parent: u8, kind_raw: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(data.len() + 10);
    frame.push(CRSF_SYNC);
    frame.push((data.len() + 7) as u8);
    frame.push(PARAM_SETTINGS_ENTRY_ID);
    frame.push(RADIO_ADDRESS);
    frame.push(MODULE_ADDRESS);
    frame.push(field_id);
    frame.push(0);
    frame.push(parent);
    frame.push(kind_raw);
    frame.extend_from_slice(data);
    frame.push(0);
    frame
}

fn is_pending_wifi_start_frame(pending: Option<&PendingCommand>, frame: &[u8]) -> bool {
    let Some(PendingCommand {
        field_id,
        action: PendingCommandAction::WifiEnable,
    }) = pending
    else {
        return false;
    };
    frame.get(2).copied() == Some(PARAM_WRITE_ID)
        && frame.get(5).copied() == Some(*field_id)
        && frame.get(6).copied() == Some(CommandStep::Start.raw())
}

fn parameter_handset_address(destination: u8) -> u8 {
    if destination == MODULE_ADDRESS {
        ELRS_HANDSET_ADDRESS
    } else {
        RADIO_ADDRESS
    }
}

fn build_extended_frame(frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 6);
    frame.push(CRSF_SYNC);
    frame.push((payload.len() + 2) as u8);
    frame.push(frame_type);
    frame.extend_from_slice(payload);
    frame.push(CRSF_D5_CRC.checksum(&frame[2..]));
    frame
}

fn parse_device_info(frame: &[u8]) -> Option<DeviceInfo> {
    if frame.len() < 10
        || frame[2] != PARAM_DEVICE_INFO_ID
        || frame.get(4).copied()? != MODULE_ADDRESS
    {
        return None;
    }
    let name_size = frame[1].checked_sub(18)? as usize;
    let name_end = 5usize.checked_add(name_size)?;
    let name = String::from_utf8_lossy(frame.get(5..name_end)?).to_string();
    let is_elrs = frame.get(name_end..name_end + 4) == Some(b"ELRS".as_slice());
    let major = frame.get(14 + name_size).copied().unwrap_or_default() as u32;
    let minor = frame.get(15 + name_size).copied().unwrap_or_default() as u32;
    let revision = frame.get(16 + name_size).copied().unwrap_or_default() as u32;
    // param_count 在 name + serial(4) + hw(4) + sw(4) 之后的字节
    // 即 frame[name_end + 12]
    let param_count = frame
        .get(name_end + 12)
        .copied()
        .filter(|&c| c > 0)
        .unwrap_or(PARAM_MAX_FIELD_ID);
    Some(DeviceInfo {
        is_elrs,
        name,
        serial: 0,
        hw_version: major,
        sw_version: (major << 16) | (minor << 8) | revision,
        field_count: param_count,
    })
}

fn parse_parameter_entry(frame: &[u8]) -> Option<ParamEntry> {
    if frame.len() < 11 || frame[2] != PARAM_SETTINGS_ENTRY_ID {
        return None;
    }
    let field_id = *frame.get(5)?;
    let chunks_remain = *frame.get(6)?;
    let parent = *frame.get(7)?;
    let kind_raw = *frame.get(8)?;
    let kind = parse_param_kind(kind_raw);
    let mut cursor = 9usize;
    // ELRS 使用 8 字节数据块。COMMAND 类型首 chunk 的 label 末尾可能落在下一个 chunk
    // 中，导致本帧内找不到 null 终止符。此时用所有可用字节作截断 label 存入 params，
    // find_wifi_command_fallback 用前缀匹配仍可识别（"enable wifi".starts_with("enable w")）。
    let label_terminated = frame[cursor..].iter().position(|byte| *byte == 0);
    let label_end = match label_terminated {
        Some(pos) => pos + cursor,
        None => {
            if chunks_remain != 0
                && (matches!(kind, ParamKind::Command) || matches!(kind, ParamKind::Select))
            {
                frame.len().saturating_sub(1) // CRC 之前的所有字节作截断 label
            } else {
                return None;
            }
        }
    };
    let label = String::from_utf8_lossy(&frame[cursor..label_end]).to_string();
    cursor = if label_terminated.is_some() {
        label_end + 1
    } else {
        frame.len().saturating_sub(1)
    };

    // 默认要求完整 chunk。仅 COMMAND 和 TX power 的 SELECT 允许使用首 chunk 的截断信息。
    if chunks_remain != 0
        && !matches!(kind, ParamKind::Command)
        && !(matches!(kind, ParamKind::Select) && is_tx_power_label_candidate(&label))
    {
        return None;
    }

    let mut options = Vec::new();
    let mut value = String::new();
    let mut command_status = None;

    match kind {
        ParamKind::Select => {
            let options_end = frame[cursor..].iter().position(|byte| *byte == 0);
            if let Some(offset) = options_end {
                let options_end = offset + cursor;
                options = parse_select_options(&frame[cursor..options_end]);
                cursor = options_end + 1;
                if let Some(current) = frame.get(cursor).copied() {
                    value = options
                        .get(current as usize)
                        .cloned()
                        .unwrap_or_else(|| current.to_string());
                } else if chunks_remain != 0 {
                    value = options.first().cloned().unwrap_or_default();
                } else {
                    return None;
                }
            } else if chunks_remain != 0 && is_tx_power_label_candidate(&label) {
                options = parse_select_options(&frame[cursor..frame.len().saturating_sub(1)]);
                value = options.first().cloned().unwrap_or_default();
            } else {
                return None;
            }
        }
        ParamKind::Command => {
            if label_terminated.is_none() {
                return Some(ParamEntry {
                    id: field_id,
                    parent,
                    kind,
                    label,
                    options,
                    value,
                    command_status,
                });
            }
            let step_raw = *frame.get(cursor)?;
            let timeout = frame.get(cursor + 1).copied().unwrap_or_default();
            let info_start = cursor.saturating_add(2);
            let info = frame
                .get(info_start..frame.len().saturating_sub(1))
                .map(|slice| {
                    let end = slice
                        .iter()
                        .position(|byte| *byte == 0)
                        .unwrap_or(slice.len());
                    String::from_utf8_lossy(&slice[..end]).to_string()
                })
                .unwrap_or_default();
            let step = CommandStep::from_raw(step_raw);
            value = if info.is_empty() {
                step.label().to_string()
            } else {
                format!("{} {}", step.label(), info)
            };
            command_status = Some(CommandStatus {
                step,
                timeout,
                info,
            });
            if chunks_remain != 0 {
                // 多 chunk COMMAND 的 info 字符串可能不完整；保留 step/timeout 语义，
                // 后续 chunk=0 的完整 entry 会覆盖。
                if let Some(status) = command_status.as_mut() {
                    status.info = status.info.trim_end_matches(char::from(0)).to_string();
                }
            }
        }
        ParamKind::Info | ParamKind::String => {
            let value_end = frame[cursor..]
                .iter()
                .position(|byte| *byte == 0)
                .map(|offset| cursor + offset)
                .unwrap_or(frame.len().saturating_sub(1));
            value = String::from_utf8_lossy(&frame[cursor..value_end]).to_string();
        }
        ParamKind::Folder | ParamKind::Unknown(_) => {}
    }

    if field_id == ELRS_LINK_STAT_ID {
        if let Some(power) =
            parse_link_stat_tx_power_mw(frame.get(3..frame.len().saturating_sub(1))?)
        {
            value = format!("{}mW", power);
        }
    }

    Some(ParamEntry {
        id: field_id,
        parent,
        kind,
        label,
        options,
        value,
        command_status,
    })
}

fn suffix_info(info: &str) -> String {
    if info.is_empty() {
        String::new()
    } else {
        format!(": {}", info)
    }
}

fn parse_param_kind(kind: u8) -> ParamKind {
    match kind & 0x5F {
        9 => ParamKind::Select,
        10 => ParamKind::String,
        11 => ParamKind::Folder,
        12 => ParamKind::Info,
        13 => ParamKind::Command,
        _ => ParamKind::Unknown(kind),
    }
}

fn nearest_option_index(options: &[String], target_mw: u16) -> Option<usize> {
    options
        .iter()
        .enumerate()
        .filter_map(|(idx, option)| parse_power_value(option).map(|value| (idx, value)))
        .min_by_key(|(_, value)| value.abs_diff(target_mw))
        .map(|(idx, _)| idx)
}

fn parse_power_value(raw: &str) -> Option<u16> {
    let digits: String = raw.chars().filter(|ch| ch.is_ascii_digit()).collect();
    digits.parse::<u16>().ok()
}

fn parse_link_stat_tx_power_mw(raw: &[u8]) -> Option<u16> {
    let idx = *raw.get(7)? as usize;
    const POWER_VALUES: [u16; 9] = [0, 10, 25, 100, 500, 1000, 2000, 250, 50];
    POWER_VALUES.get(idx).copied()
}

fn parse_link_frame_tx_power_mw(frame: &[u8]) -> Option<u16> {
    let idx = *frame.get(9)? as usize;
    const POWER_VALUES: [u16; 9] = [0, 10, 25, 100, 500, 1000, 2000, 250, 50];
    POWER_VALUES.get(idx).copied()
}

fn fallback_tx_power_option_index(target_mw: u16) -> usize {
    TX_POWER_FALLBACK_LEVELS_MW
        .iter()
        .enumerate()
        .min_by_key(|(_, value)| value.abs_diff(target_mw))
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn parse_select_options(raw: &[u8]) -> Vec<String> {
    raw.split(|byte| *byte == b';')
        .map(|slice| String::from_utf8_lossy(slice).trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn tx_power_label_priority(label: &str) -> Option<usize> {
    let lower = label.to_ascii_lowercase();
    TX_POWER_FALLBACK_LABELS.iter().position(|candidate| {
        lower == *candidate || (lower.len() >= 4 && candidate.starts_with(lower.as_str()))
    })
}

fn current_tx_power_label_priority(label: &str) -> Option<usize> {
    let lower = label.to_ascii_lowercase();
    TX_POWER_CURRENT_FALLBACK_LABELS
        .iter()
        .position(|candidate| {
            lower == *candidate || (lower.len() >= 4 && candidate.starts_with(lower.as_str()))
        })
}

fn is_tx_power_label_candidate(label: &str) -> bool {
    tx_power_label_priority(label).is_some()
}

fn crc8_ba(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in data {
        crc ^= *byte;
        for _ in 0..8 {
            if (crc & 0x80) != 0 {
                crc = (crc << 1) ^ 0xBA;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::{
        build_crossfire_bind_frame, build_crossfire_model_id_frame, build_parameter_read_frame,
        build_parameter_write_frame, crc8_ba, parse_parameter_entry, CommandStep, ElrsOperation,
        ElrsProtocolRuntime, ParamEntry, ParamKind, ELRS_HANDSET_ADDRESS, MODULE_ADDRESS,
        PARAM_SETTINGS_ENTRY_ID,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn test_crc8_ba_known_value() {
        assert_eq!(crc8_ba(&[0x32, 0xEE, 0xEA, 0x10, 0x01]), 0x14);
    }

    #[test]
    fn test_build_crossfire_bind_frame_matches_expected_layout() {
        assert_eq!(
            build_crossfire_bind_frame(MODULE_ADDRESS),
            vec![0xC8, 0x07, 0x32, 0xEE, 0xEA, 0x10, 0x01, 0x14, 0xEB]
        );
    }

    #[test]
    fn test_build_crossfire_model_id_frame_matches_expected_layout() {
        assert_eq!(
            build_crossfire_model_id_frame(MODULE_ADDRESS, 0),
            vec![0xC8, 0x08, 0x32, 0xEE, 0xEA, 0x10, 0x05, 0x00, 0xDC, 0x4D]
        );
    }

    #[test]
    fn test_bind_request_queues_command_when_available() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            17,
            ParamEntry {
                id: 17,
                parent: 0,
                kind: ParamKind::Command,
                label: "Bind".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );
        let status = runtime.request(ElrsOperation::EnterBind);
        assert_eq!(status.message(), "Bind command queued");
        assert!(runtime.bind_active());
    }

    #[test]
    fn test_parse_select_parameter_entry() {
        let frame = vec![
            0xC8,
            0x18,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x07,
            0x00,
            0x06,
            0x09,
            b'M',
            b'a',
            b'x',
            b' ',
            b'P',
            b'o',
            b'w',
            b'e',
            b'r',
            0,
            b'1',
            b'0',
            b';',
            b'2',
            b'5',
            0,
            0x01,
            0x00,
            0x02,
            0x00,
        ];
        let entry = parse_parameter_entry(&frame).expect("entry");
        assert_eq!(entry.label, "Max Power");
        assert_eq!(entry.options, vec!["10".to_string(), "25".to_string()]);
        assert_eq!(entry.value, "25");
    }

    #[test]
    fn test_parse_command_parameter_entry_extracts_step_timeout_and_info() {
        let frame = vec![
            0xC8,
            0x19,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x0F,
            0x00,
            0x00,
            0x0D,
            b'E',
            b'n',
            b'a',
            b'b',
            b'l',
            b'e',
            b' ',
            b'W',
            b'i',
            b'F',
            b'i',
            0x00,
            0x06,
            0x14,
            b'W',
            b'a',
            b'i',
            b't',
            0x00,
            0x00,
        ];
        let entry = parse_parameter_entry(&frame).expect("command entry");
        let status = entry.command_status.expect("command status");
        assert_eq!(entry.label, "Enable WiFi");
        assert_eq!(status.step, CommandStep::Poll);
        assert_eq!(status.timeout, 0x14);
        assert_eq!(status.info, "Wait");
        assert_eq!(entry.value, "POLL Wait");
    }

    #[test]
    fn test_parse_multichunk_command_entry_keeps_truncated_label() {
        let frame = vec![
            0xC8,
            0x11,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x0F,
            0x01,
            0x00,
            0x0D,
            b'E',
            b'n',
            b'a',
            b'b',
            b'l',
            b'e',
            b' ',
            b'W',
            0xAA,
        ];
        let entry = parse_parameter_entry(&frame).expect("multichunk command entry");
        assert_eq!(entry.label, "Enable W");
        assert!(entry.command_status.is_none());
    }

    #[test]
    fn test_build_parameter_read_frame_uses_device_then_handset() {
        assert_eq!(
            build_parameter_read_frame(MODULE_ADDRESS, 0x3E, 0),
            vec![0xC8, 0x06, 0x2C, 0xEE, 0xEF, 0x3E, 0x00, 0x1A]
        );
    }

    #[test]
    fn test_build_parameter_write_frame_contains_field_id_and_value() {
        let frame = build_parameter_write_frame(MODULE_ADDRESS, 7, &[2]);
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[3], MODULE_ADDRESS);
        assert_eq!(frame[4], ELRS_HANDSET_ADDRESS);
        assert_eq!(frame[5], 7);
        assert_eq!(frame[6], 2);
    }

    #[test]
    fn test_bind_active_stays_true_during_settle_window() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            17,
            ParamEntry {
                id: 17,
                parent: 0,
                kind: ParamKind::Command,
                label: "Bind".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );
        runtime.request(ElrsOperation::EnterBind);
        let frame = runtime.poll_outgoing_frame(Instant::now());
        assert!(frame.is_some());
        assert!(runtime.bind_active());
    }

    #[test]
    fn test_model_id_update_changes_next_model_id_frame() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.set_model_id(7);
        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("model id frame");
        assert_eq!(frame, build_crossfire_model_id_frame(MODULE_ADDRESS, 7));
    }

    #[test]
    fn test_bind_phrase_write_queues_string_param_when_available() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            9,
            ParamEntry {
                id: 9,
                parent: 0,
                kind: ParamKind::String,
                label: "Bind Phrase".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );
        let status = runtime.request(ElrsOperation::SetBindPhrase("abc123".to_string()));
        assert_eq!(status.message(), "Bind phrase write queued");
        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued frame");
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[5], 9);
        assert_eq!(&frame[6..12], b"abc123");
    }

    #[test]
    fn test_select_tx_power_value_prefers_current_over_max() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            3,
            ParamEntry {
                id: 3,
                parent: 0,
                kind: ParamKind::Select,
                label: "Max Power".to_string(),
                options: vec!["25".to_string(), "250".to_string()],
                value: "250".to_string(),
                command_status: None,
            },
        );
        runtime.params.insert(
            4,
            ParamEntry {
                id: 4,
                parent: 0,
                kind: ParamKind::Select,
                label: "Power".to_string(),
                options: vec!["10".to_string(), "100".to_string()],
                value: "100".to_string(),
                command_status: None,
            },
        );

        assert_eq!(runtime.select_tx_power_value().as_deref(), Some("100"));
    }

    #[test]
    fn test_set_tx_power_prefers_current_power_field_id() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            9,
            ParamEntry {
                id: 9,
                parent: 0,
                kind: ParamKind::Select,
                label: "Max Power".to_string(),
                options: vec!["25".to_string(), "250".to_string()],
                value: "25".to_string(),
                command_status: None,
            },
        );
        runtime.params.insert(
            10,
            ParamEntry {
                id: 10,
                parent: 0,
                kind: ParamKind::Select,
                label: "Power".to_string(),
                options: vec!["25".to_string(), "250".to_string()],
                value: "25".to_string(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetTxPower(250));
        assert_eq!(status.message(), "TX power request queued");

        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued tx power frame");
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[5], 10);
    }

    #[test]
    fn test_set_tx_power_prefers_label_priority_over_hashmap_iteration_order() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            2,
            ParamEntry {
                id: 2,
                parent: 0,
                kind: ParamKind::Select,
                label: "RF Power".to_string(),
                options: vec!["25".to_string(), "250".to_string()],
                value: "25".to_string(),
                command_status: None,
            },
        );
        runtime.params.insert(
            20,
            ParamEntry {
                id: 20,
                parent: 0,
                kind: ParamKind::Select,
                label: "Power".to_string(),
                options: vec!["25".to_string(), "250".to_string()],
                value: "25".to_string(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetTxPower(250));
        assert_eq!(status.message(), "TX power request queued");

        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued tx power frame");
        assert_eq!(frame[2], 0x2D);
        // "Power" is ahead of "RF Power" in TX_POWER_CURRENT_LABELS.
        assert_eq!(frame[5], 20);
    }

    #[test]
    fn test_set_tx_power_falls_back_to_max_power_when_current_missing() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            11,
            ParamEntry {
                id: 11,
                parent: 0,
                kind: ParamKind::Select,
                label: "Max Power".to_string(),
                options: vec!["25".to_string(), "500".to_string()],
                value: "25".to_string(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetTxPower(500));
        assert_eq!(status.message(), "TX power request queued");

        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued tx power frame");
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[5], 11);
    }

    #[test]
    fn test_set_tx_power_returns_unsupported_when_no_power_select_found() {
        let mut runtime = ElrsProtocolRuntime::default();
        let status = runtime.request(ElrsOperation::SetTxPower(250));
        assert!(matches!(status, super::ElrsOperationStatus::Unsupported(_)));
        assert_eq!(
            status.message(),
            "TX power is read-only on this module (adjust TX max power)"
        );
    }

    #[test]
    fn test_set_tx_power_rejected_when_only_max_power_exists() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            6,
            ParamEntry {
                id: 6,
                parent: 0,
                kind: ParamKind::Select,
                label: "Max Power".to_string(),
                options: vec!["10".to_string(), "25".to_string()],
                value: "10".to_string(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetTxPower(25));
        assert!(matches!(status, super::ElrsOperationStatus::Unsupported(_)));
        assert_eq!(
            status.message(),
            "TX power is read-only on this module (adjust TX max power)"
        );
    }

    #[test]
    fn test_set_tx_power_uses_fallback_index_when_tx_power_select_chunk_has_no_options_yet() {
        let mut runtime = ElrsProtocolRuntime::default();
        let first_chunk = vec![
            0xC8,
            0x11,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x05,
            0x01,
            0x00,
            0x09,
            b'T',
            b'X',
            b' ',
            b'P',
            b'o',
            b'w',
            b'e',
            b'r',
            0x00,
        ];
        runtime.consume_frame(&first_chunk);

        let status = runtime.request(ElrsOperation::SetTxPower(100));
        assert_eq!(status.message(), "TX power request queued");

        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued tx power frame");
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[5], 0x05);
        // fallback levels: [10, 25, 50, 100, 250, 500, 1000, 2000]
        assert_eq!(frame[6], 3);
    }

    #[test]
    fn test_tx_power_write_gets_applied_feedback_on_matching_select_update() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            5,
            ParamEntry {
                id: 5,
                parent: 0,
                kind: ParamKind::Select,
                label: "TX Power".to_string(),
                options: vec!["10".to_string(), "25".to_string(), "100".to_string()],
                value: "10".to_string(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetTxPower(100));
        assert_eq!(status.message(), "TX power request queued");

        let _frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued tx power frame");

        let confirm = vec![
            0xC8,
            0x1C,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x05,
            0x00,
            0x00,
            0x09,
            b'T',
            b'X',
            b' ',
            b'P',
            b'o',
            b'w',
            b'e',
            b'r',
            0x00,
            b'1',
            b'0',
            b';',
            b'2',
            b'5',
            b';',
            b'1',
            b'0',
            b'0',
            0x00,
            0x02,
        ];
        runtime.consume_frame(&confirm);
        let status_text = runtime.take_status_text();
        assert_eq!(status_text.as_deref(), Some("TX power applied: 100"));
    }

    #[test]
    fn test_tx_max_power_write_gets_applied_feedback_on_matching_select_update() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            6,
            ParamEntry {
                id: 6,
                parent: 0,
                kind: ParamKind::Select,
                label: "Max Power".to_string(),
                options: vec!["25".to_string(), "250".to_string(), "500".to_string()],
                value: "250".to_string(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetTxMaxPower(500));
        assert_eq!(status.message(), "TX max power request queued");

        let _frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued tx max power frame");

        let confirm = vec![
            0xC8, 0x1D, 0x2B, 0xEA, 0xEE, 0x06, 0x00, 0x00, 0x09, b'M', b'a', b'x', b' ', b'P',
            b'o', b'w', b'e', b'r', 0, b'2', b'5', b';', b'2', b'5', b'0', b';', b'5', b'0', b'0',
            0, 0x02,
        ];
        runtime.consume_frame(&confirm);
        let status_text = runtime.take_status_text();
        assert_eq!(status_text.as_deref(), Some("TX max power applied: 500"));
    }

    #[test]
    fn test_tx_power_write_gets_applied_feedback_on_2d_ack() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.params.insert(
            6,
            ParamEntry {
                id: 6,
                parent: 0,
                kind: ParamKind::Select,
                label: "Max Power".to_string(),
                options: vec!["25".to_string(), "250".to_string(), "500".to_string()],
                value: "25".to_string(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetTxMaxPower(500));
        assert_eq!(status.message(), "TX max power request queued");

        let _frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("queued tx max power frame");

        let ack = vec![0xC8, 0x06, 0x2D, 0xEE, 0xEF, 0x06, 0x02, 0x00];
        runtime.consume_frame(&ack);
        let status_text = runtime.take_status_text();
        assert_eq!(status_text.as_deref(), Some("TX max power applied: idx 2"));
    }

    #[test]
    fn test_tx_power_confirmation_via_link_stat_telemetry() {
        let mut runtime = ElrsProtocolRuntime::default();
        let first_chunk = vec![
            0xC8, 0x11, 0x2B, 0xEA, 0xEE, 0x05, 0x01, 0x00, 0x09, b'T', b'X', b' ', b'P', b'o',
            b'w', b'e', b'r', 0x00,
        ];
        runtime.consume_frame(&first_chunk);

        let status = runtime.request(ElrsOperation::SetTxPower(100));
        assert_eq!(status.message(), "TX power request queued");

        let _write = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_millis(100))
            .expect("tx power write frame");

        let link_stat = vec![
            0xC8, 0x0C, 0x14, 0xEA, 0xEE, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
        ];
        runtime.consume_frame(&link_stat);
        let status_text = runtime.take_status_text();
        assert_eq!(status_text.as_deref(), Some("TX power applied: 100mW"));
    }

    #[test]
    fn test_wifi_enable_returns_unsupported_when_module_not_found() {
        // device_info is None → module not yet discovered → Unsupported + refresh triggered
        let mut runtime = ElrsProtocolRuntime::default();
        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert!(
            matches!(status, super::ElrsOperationStatus::Unsupported(_)),
            "should be Unsupported when module not yet discovered, got {:?}",
            status
        );
        // request_refresh should have been triggered: param_refresh_requested reset via ping
        // (we just verify the queue is non-empty or ping will fire soon via last_ping_at=None)
        let frame = runtime.poll_outgoing_frame(Instant::now() + Duration::from_secs(1));
        assert!(
            frame.is_some(),
            "refresh ping should be queued after Unsupported"
        );
    }

    #[test]
    fn test_wifi_enable_returns_busy_when_module_known_but_params_not_enumerated() {
        // device_info is Some but WiFi command field not yet in params → Busy, no refresh
        let mut runtime = ElrsProtocolRuntime::default();
        // Inject a minimal DeviceInfo so the module is "known"
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 64,
            is_elrs: true,
        });
        let params_before = runtime.params.len();
        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert!(
            matches!(status, super::ElrsOperationStatus::Busy(_)),
            "should be Busy when module known but WiFi field not yet enumerated, got {:?}",
            status
        );
        // Crucially: params must NOT have been cleared (no request_refresh)
        assert_eq!(
            runtime.params.len(),
            params_before,
            "params should not be cleared when returning Busy"
        );
    }

    #[test]
    fn test_wifi_enable_queues_frame_when_field_found() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 64,
            is_elrs: true,
        });
        runtime.params.insert(
            5,
            ParamEntry {
                id: 5,
                parent: 0,
                kind: ParamKind::Command,
                label: "Enable WiFi".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );
        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert_eq!(status.message(), "WiFi command queued");
        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("WiFi command frame should be queued");
        // C8 06 2D EE EF <field_id=5> <value=1> <crc>
        assert_eq!(frame[2], 0x2D, "frame type must be PARAM_WRITE");
        assert_eq!(frame[3], MODULE_ADDRESS, "dest must be MODULE_ADDRESS");
        assert_eq!(
            frame[4], ELRS_HANDSET_ADDRESS,
            "src must be ELRS_HANDSET_ADDRESS"
        );
        assert_eq!(frame[5], 5, "field_id must match discovered WiFi field");
        assert_eq!(frame[6], 1, "value must be 1 (COMMAND CLICK)");
    }

    #[test]
    fn test_wifi_enable_allows_followup_protocol_traffic_for_recovery() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 19,
            is_elrs: true,
        });
        runtime.params.insert(
            15,
            ParamEntry {
                id: 15,
                parent: 0,
                kind: ParamKind::Command,
                label: "Enable WiFi".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );
        runtime.request_refresh();

        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert_eq!(status.message(), "WiFi command queued");

        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("WiFi command frame should be queued");
        assert_eq!(frame[2], 0x2D, "first frame must be the WiFi PARAM_WRITE");

        let followup = runtime.poll_outgoing_frame(Instant::now() + Duration::from_secs(2));
        assert!(
            followup.is_some(),
            "runtime should continue protocol recovery traffic after WiFi command"
        );
    }

    #[test]
    fn test_wifi_enable_poll_status_queues_followup_poll_frame() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 19,
            is_elrs: true,
        });
        runtime.params.insert(
            15,
            ParamEntry {
                id: 15,
                parent: 0,
                kind: ParamKind::Command,
                label: "Enable WiFi".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );

        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert_eq!(status.message(), "WiFi command queued");

        let start = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("start frame");
        assert_eq!(start[2], 0x2D);
        assert_eq!(start[6], CommandStep::Start.raw());

        let poll_status = vec![
            0xC8,
            0x18,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x0F,
            0x00,
            0x00,
            0x0D,
            b'E',
            b'n',
            b'a',
            b'b',
            b'l',
            b'e',
            b' ',
            b'W',
            b'i',
            b'F',
            b'i',
            0x00,
            0x06,
            0x05,
            b'W',
            b'a',
            b'i',
            b't',
            0x00,
        ];
        runtime.consume_frame(&poll_status);

        let poll = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(2))
            .expect("poll frame");
        assert_eq!(poll[2], 0x2D);
        assert_eq!(poll[5], 0x0F);
        assert_eq!(poll[6], CommandStep::Poll.raw());
    }

    #[test]
    fn test_wifi_enable_confirmation_status_queues_confirm_frame() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 19,
            is_elrs: true,
        });
        runtime.params.insert(
            15,
            ParamEntry {
                id: 15,
                parent: 0,
                kind: ParamKind::Command,
                label: "Enable WiFi".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );

        runtime.request(ElrsOperation::SetWifiManual(true));
        let _ = runtime.poll_outgoing_frame(Instant::now() + Duration::from_secs(1));

        let confirm_status = vec![
            0xC8,
            0x1D,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x0F,
            0x00,
            0x00,
            0x0D,
            b'E',
            b'n',
            b'a',
            b'b',
            b'l',
            b'e',
            b' ',
            b'W',
            b'i',
            b'F',
            b'i',
            0x00,
            0x03,
            0x05,
            b'C',
            b'o',
            b'n',
            b'f',
            b'i',
            b'r',
            b'm',
            0x00,
            0x00,
        ];
        runtime.consume_frame(&confirm_status);

        let confirm = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(2))
            .expect("confirm frame");
        assert_eq!(confirm[2], 0x2D);
        assert_eq!(confirm[5], 0x0F);
        assert_eq!(confirm[6], CommandStep::Confirm.raw());
    }

    #[test]
    fn test_wifi_disable_returns_queued_without_sending_frame() {
        // SetWifiManual(false) should return Queued immediately, no frame enqueued
        let mut runtime = ElrsProtocolRuntime::default();
        let status = runtime.request(ElrsOperation::SetWifiManual(false));
        assert_eq!(status.message(), "WiFi state cleared");
        // No frame should be queued
        let frame = runtime.poll_outgoing_frame(Instant::now() + Duration::from_secs(1));
        // model_id frame will fire (model_id_pending=true by default), so drain it
        // The important thing is no 0x2D PARAM_WRITE frame for WiFi disable
        if let Some(f) = frame {
            assert_ne!(
                f[2], 0x2D,
                "disable WiFi must not enqueue a PARAM_WRITE frame"
            );
        }
    }

    #[test]
    fn test_multichunk_wifi_command_remains_usable_after_final_chunk() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 19,
            is_elrs: true,
        });

        let first = vec![
            0xC8,
            0x11,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x0F,
            0x01,
            0x00,
            0x0D,
            b'E',
            b'n',
            b'a',
            b'b',
            b'l',
            b'e',
            b' ',
            b'W',
            0x00,
        ];
        let second = vec![
            0xC8,
            0x0E,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x0F,
            0x00,
            0x00,
            0x0D,
            b'i',
            b'F',
            b'i',
            0x00,
            0x00,
            0x00,
        ];

        runtime.consume_frame(&first);
        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert_eq!(status.message(), "WiFi command queued");
        let first_frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("first wifi command");
        assert_eq!(first_frame[2], 0x2D);
        assert_eq!(first_frame[5], 0x0F);
        assert_eq!(first_frame[6], CommandStep::Start.raw());

        runtime.consume_frame(&second);
        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert_eq!(status.message(), "WiFi command queued");
        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(2))
            .expect("wifi command");
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[5], 0x0F);
        assert_eq!(frame[6], CommandStep::Start.raw());
    }

    #[test]
    fn test_pending_wifi_first_chunk_is_usable_as_fallback_candidate() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 19,
            is_elrs: true,
        });

        let first = vec![
            0xC8,
            0x11,
            PARAM_SETTINGS_ENTRY_ID,
            0xEA,
            0xEE,
            0x0F,
            0x01,
            0x00,
            0x0D,
            b'E',
            b'n',
            b'a',
            b'b',
            b'l',
            b'e',
            b' ',
            b'W',
            0x00,
        ];
        runtime.consume_frame(&first);

        let status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert_eq!(status.message(), "WiFi command queued");
        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("wifi command");
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[5], 0x0F);
        assert_eq!(frame[6], CommandStep::Start.raw());
    }

    #[test]
    fn test_wifi_pending_retries_start_frame_periodically() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 19,
            is_elrs: true,
        });
        runtime.params.insert(
            15,
            ParamEntry {
                id: 15,
                parent: 0,
                kind: ParamKind::Command,
                label: "Enable WiFi".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );

        runtime.request(ElrsOperation::SetWifiManual(true));
        let first = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("first start");
        assert_eq!(first[2], 0x2D);
        assert_eq!(first[5], 0x0F);
        assert_eq!(first[6], CommandStep::Start.raw());

        let retry = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(2))
            .expect("retry start");
        assert_eq!(retry[2], 0x2D);
        assert_eq!(retry[5], 0x0F);
        assert_eq!(retry[6], CommandStep::Start.raw());
    }

    #[test]
    fn test_wifi_disable_clears_queued_start_frames() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 19,
            is_elrs: true,
        });
        runtime.params.insert(
            15,
            ParamEntry {
                id: 15,
                parent: 0,
                kind: ParamKind::Command,
                label: "Enable WiFi".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );

        runtime.request(ElrsOperation::SetWifiManual(true));
        let status = runtime.request(ElrsOperation::SetWifiManual(false));
        assert_eq!(status.message(), "WiFi state cleared");

        let frame = runtime.poll_outgoing_frame(Instant::now() + Duration::from_secs(1));
        assert!(
            frame
                .map(|f| f[2] != 0x2D || f[5] != 0x0F || f[6] != CommandStep::Start.raw())
                .unwrap_or(true),
            "WiFi off should remove queued START frames"
        );
    }

    #[test]
    fn test_wifi_disable_does_not_drop_unrelated_param_write_frames() {
        let mut runtime = ElrsProtocolRuntime::default();
        runtime.device_info = Some(super::DeviceInfo {
            name: "DuplicateTX ESP".to_string(),
            serial: 0,
            hw_version: 3,
            sw_version: (3 << 16) | (2 << 8) | 1,
            field_count: 64,
            is_elrs: true,
        });
        runtime.params.insert(
            15,
            ParamEntry {
                id: 15,
                parent: 0,
                kind: ParamKind::Command,
                label: "Enable WiFi".to_string(),
                options: Vec::new(),
                value: String::new(),
                command_status: None,
            },
        );
        runtime.params.insert(
            7,
            ParamEntry {
                id: 7,
                parent: 0,
                kind: ParamKind::Select,
                label: "Power".to_string(),
                options: vec!["25".to_string(), "250".to_string()],
                value: "25".to_string(),
                command_status: None,
            },
        );

        let wifi_status = runtime.request(ElrsOperation::SetWifiManual(true));
        assert_eq!(wifi_status.message(), "WiFi command queued");
        let power_status = runtime.request(ElrsOperation::SetTxPower(250));
        assert_eq!(power_status.message(), "TX power request queued");

        let clear_status = runtime.request(ElrsOperation::SetWifiManual(false));
        assert_eq!(clear_status.message(), "WiFi state cleared");

        let frame = runtime
            .poll_outgoing_frame(Instant::now() + Duration::from_secs(1))
            .expect("tx power write should remain queued");
        assert_eq!(frame[2], 0x2D);
        assert_eq!(frame[5], 7);
        assert_eq!(frame[6], 1);
    }
}
