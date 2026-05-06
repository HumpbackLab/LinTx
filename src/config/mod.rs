pub mod store;

use serde::{Deserialize, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RadioConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub active_model: String,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub input: InputConfig,
    #[serde(default)]
    pub elrs: ElrsUiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelConfig {
    #[serde(default = "default_model_id")]
    pub id: String,
    #[serde(default = "default_model_name")]
    pub name: String,
    #[serde(default)]
    pub input_mapping: InputMapping,
    #[serde(default)]
    pub mixer: MixerConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub profiles: Vec<RateProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiConfig {
    #[serde(default = "default_backlight")]
    pub backlight_percent: u8,
    #[serde(default = "default_theme")]
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioConfig {
    #[serde(default = "default_volume")]
    pub sound_percent: u8,
    #[serde(default)]
    pub mute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputConfig {
    #[serde(default)]
    pub calibration_profile: String,
    #[serde(default)]
    pub source_priority: Vec<InputSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ElrsUiConfig {
    #[serde(default)]
    pub rf_output_enabled: bool,
    #[serde(default)]
    pub wifi_manual_on: bool,
    #[serde(default)]
    pub bind_mode: bool,
    #[serde(default = "default_elrs_tx_power_mw")]
    pub tx_power_mw: u16,
    #[serde(default = "default_elrs_tx_max_power_mw")]
    pub tx_max_power_mw: u16,
    #[serde(default = "default_elrs_bind_phrase")]
    pub bind_phrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputMapping {
    #[serde(default)]
    pub channels: Vec<InputChannel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputChannel {
    pub role: ControlRole,
    #[serde(default)]
    pub source: InputSource,
    #[serde(default)]
    pub index: u8,
    #[serde(default)]
    pub reversed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MixerConfig {
    #[serde(default)]
    pub outputs: Vec<MixerOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MixerOutput {
    pub role: ControlRole,
    #[serde(default = "default_weight")]
    pub weight: i16,
    #[serde(default)]
    pub offset: i16,
    #[serde(default)]
    pub curve: CurveRef,
    #[serde(default)]
    pub limits: OutputLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputConfig {
    #[serde(default)]
    pub protocol: OutputProtocol,
    #[serde(default)]
    pub channel_order: Vec<ControlRole>,
    #[serde(default)]
    pub failsafe: Vec<i16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sensors: Vec<TelemetrySensorConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetrySensorConfig {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RateProfile {
    #[serde(default = "default_profile_name")]
    pub name: String,
    #[serde(default = "default_rate")]
    pub roll_rate: u16,
    #[serde(default = "default_rate")]
    pub pitch_rate: u16,
    #[serde(default = "default_rate")]
    pub yaw_rate: u16,
    #[serde(default = "default_expo")]
    pub expo_percent: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    Adc,
    Stm32Serial,
    Crsf,
    Joydev,
    Mock,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ControlRole {
    Thrust,
    Direction,
    Aileron,
    Elevator,
    Arm,
    #[serde(alias = "mode")]
    FlightMode,
    Beeper,
    Turtle,
    Prearm,
    GpsRescue,
    Aux1,
    Aux2,
    Aux3,
    Aux4,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CurveRef {
    #[default]
    Linear,
    Expo,
    Custom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputProtocol {
    #[default]
    Crsf,
    UsbHid,
    Ppm,
    Sbus,
}

impl OutputProtocol {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Crsf => "CRSF",
            Self::UsbHid => "USB HID",
            Self::Ppm => "PPM",
            Self::Sbus => "SBUS",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputLimits {
    #[serde(default = "default_limit_min")]
    pub min: i16,
    #[serde(default = "default_limit_max")]
    pub max: i16,
    #[serde(default)]
    pub subtrim: i16,
    #[serde(default)]
    pub reversed: bool,
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            active_model: default_model_id(),
            ui: UiConfig::default(),
            audio: AudioConfig::default(),
            input: InputConfig::default(),
            elrs: ElrsUiConfig::default(),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            id: default_model_id(),
            name: default_model_name(),
            input_mapping: InputMapping::default(),
            mixer: MixerConfig::default(),
            output: OutputConfig::default(),
            telemetry: TelemetryConfig::default(),
            profiles: vec![RateProfile::default()],
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            backlight_percent: default_backlight(),
            theme: default_theme(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sound_percent: default_volume(),
            mute: false,
        }
    }
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            calibration_profile: "joystick.toml".to_string(),
            source_priority: vec![InputSource::Adc, InputSource::Crsf, InputSource::Mock],
        }
    }
}

impl Default for ElrsUiConfig {
    fn default() -> Self {
        Self {
            rf_output_enabled: false,
            wifi_manual_on: false,
            bind_mode: false,
            tx_power_mw: default_elrs_tx_power_mw(),
            tx_max_power_mw: default_elrs_tx_max_power_mw(),
            bind_phrase: default_elrs_bind_phrase(),
        }
    }
}

fn default_elrs_bind_phrase() -> String {
    "654321".to_string()
}

impl Default for InputMapping {
    fn default() -> Self {
        Self {
            channels: vec![
                InputChannel {
                    role: ControlRole::Thrust,
                    source: InputSource::Adc,
                    index: 0,
                    reversed: false,
                },
                InputChannel {
                    role: ControlRole::Direction,
                    source: InputSource::Adc,
                    index: 1,
                    reversed: false,
                },
                InputChannel {
                    role: ControlRole::Aileron,
                    source: InputSource::Adc,
                    index: 2,
                    reversed: false,
                },
                InputChannel {
                    role: ControlRole::Elevator,
                    source: InputSource::Adc,
                    index: 3,
                    reversed: false,
                },
            ],
        }
    }
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            outputs: vec![
                MixerOutput::new(ControlRole::Thrust),
                MixerOutput::new(ControlRole::Direction),
                MixerOutput::new(ControlRole::Aileron),
                MixerOutput::new(ControlRole::Elevator),
                MixerOutput::new(ControlRole::Arm),
                MixerOutput::new(ControlRole::FlightMode),
                MixerOutput::new(ControlRole::Beeper),
                MixerOutput::new(ControlRole::Turtle),
                MixerOutput::new(ControlRole::Prearm),
                MixerOutput::new(ControlRole::GpsRescue),
                MixerOutput::new(ControlRole::Aux1),
                MixerOutput::new(ControlRole::Aux2),
            ],
        }
    }
}

impl MixerOutput {
    pub fn new(role: ControlRole) -> Self {
        Self {
            role,
            weight: default_weight(),
            offset: 0,
            curve: CurveRef::Linear,
            limits: OutputLimits::default(),
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            protocol: OutputProtocol::Crsf,
            channel_order: vec![
                ControlRole::Aileron,
                ControlRole::Elevator,
                ControlRole::Thrust,
                ControlRole::Direction,
                ControlRole::Arm,
                ControlRole::FlightMode,
                ControlRole::Beeper,
                ControlRole::Turtle,
                ControlRole::Prearm,
                ControlRole::GpsRescue,
                ControlRole::Aux1,
                ControlRole::Aux2,
            ],
            failsafe: vec![0; 12],
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sensors: vec![
                TelemetrySensorConfig {
                    key: "rssi".to_string(),
                    unit: "percent".to_string(),
                    enabled: true,
                },
                TelemetrySensorConfig {
                    key: "link_quality".to_string(),
                    unit: "percent".to_string(),
                    enabled: true,
                },
                TelemetrySensorConfig {
                    key: "remote_battery".to_string(),
                    unit: "percent".to_string(),
                    enabled: true,
                },
            ],
        }
    }
}

impl Default for RateProfile {
    fn default() -> Self {
        Self {
            name: default_profile_name(),
            roll_rate: default_rate(),
            pitch_rate: default_rate(),
            yaw_rate: default_rate(),
            expo_percent: default_expo(),
        }
    }
}

impl Default for OutputLimits {
    fn default() -> Self {
        Self {
            min: default_limit_min(),
            max: default_limit_max(),
            subtrim: 0,
            reversed: false,
        }
    }
}

fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

fn default_model_id() -> String {
    "default".to_string()
}

fn default_model_name() -> String {
    "Default Model".to_string()
}

fn default_theme() -> String {
    "classic".to_string()
}

fn default_profile_name() -> String {
    "default".to_string()
}

fn default_backlight() -> u8 {
    70
}

fn default_volume() -> u8 {
    60
}

fn default_weight() -> i16 {
    100
}

fn default_rate() -> u16 {
    100
}

fn default_expo() -> u8 {
    0
}

fn default_limit_min() -> i16 {
    -1000
}

fn default_limit_max() -> i16 {
    1000
}

fn default_elrs_tx_power_mw() -> u16 {
    100
}

fn default_elrs_tx_max_power_mw() -> u16 {
    1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model_config_has_primary_control_roles() {
        let config = ModelConfig::default();
        let roles: Vec<ControlRole> = config
            .input_mapping
            .channels
            .iter()
            .map(|channel| channel.role)
            .collect();
        assert_eq!(
            roles,
            vec![
                ControlRole::Thrust,
                ControlRole::Direction,
                ControlRole::Aileron,
                ControlRole::Elevator,
            ]
        );
        assert_eq!(config.mixer.outputs.len(), 12);
    }

    #[test]
    fn test_radio_and_model_config_roundtrip_toml() {
        let radio = RadioConfig::default();
        let model = ModelConfig::default();

        let radio_toml = toml::to_string(&radio).unwrap();
        let model_toml = toml::to_string(&model).unwrap();

        let restored_radio: RadioConfig = toml::from_str(&radio_toml).unwrap();
        let restored_model: ModelConfig = toml::from_str(&model_toml).unwrap();

        assert_eq!(radio, restored_radio);
        assert_eq!(model, restored_model);
    }

    #[test]
    fn test_elrs_default_tx_max_power() {
        let cfg = ElrsUiConfig::default();
        assert_eq!(cfg.tx_max_power_mw, 1000);
        assert!(cfg.tx_power_mw <= cfg.tx_max_power_mw);
    }

    #[test]
    fn test_control_role_aliases_keep_old_toml_compatible() {
        #[derive(Deserialize)]
        struct RoleHolder {
            role: ControlRole,
        }

        assert_eq!(
            toml::from_str::<RoleHolder>("role = \"mode\"")
                .unwrap()
                .role,
            ControlRole::FlightMode
        );
        assert_eq!(
            toml::from_str::<RoleHolder>("role = \"aux1\"")
                .unwrap()
                .role,
            ControlRole::Aux1
        );
    }

    #[test]
    fn test_old_four_channel_model_config_reads_without_panicking() {
        let toml = r#"
id = "legacy"
name = "Legacy"

[[mixer.outputs]]
role = "thrust"

[[mixer.outputs]]
role = "direction"

[[mixer.outputs]]
role = "aileron"

[[mixer.outputs]]
role = "elevator"
"#;

        let config: ModelConfig = toml::from_str(toml).unwrap();

        assert_eq!(config.mixer.outputs.len(), 4);
        assert_eq!(config.output.channel_order.len(), 12);
    }
}
