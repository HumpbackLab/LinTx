use std::{
    fs,
    io::Read,
    sync::{Arc, Mutex},
};

use rpos::thread_logln;

use crate::{
    config::{store, AuxSource, ControlRole, ModelConfig, OutputLimits},
    input::calibrate::{
        CalibrationData,
        JoystickChannel::{self, *},
    },
    messages::{ActiveModelMsg, InputFrameMsg, RcInputRawMsg},
    CALIBRATE_FILENAME,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixerOutMsg {
    pub channels: [u16; 16],
}

impl Default for MixerOutMsg {
    fn default() -> Self {
        let mut channels = [0u16; 16];
        channels[0] = 5000;
        channels[1] = 5000;
        channels[3] = 5000;
        Self { channels }
    }
}

const THROTTLE_ARM_MAX: u16 = 500;

pub fn two_pos_to_mixer(value: bool) -> u16 {
    if value {
        10000
    } else {
        0
    }
}

pub fn three_pos_to_mixer(value: u8) -> u16 {
    match value {
        0 => 0,
        1 => 5000,
        _ => 10000,
    }
}

#[cfg(test)]
fn rc_input_from_axes(axes: [i16; 4]) -> RcInputRawMsg {
    RcInputRawMsg {
        axes,
        ..RcInputRawMsg::default()
    }
}

#[cfg(test)]
fn build_default_mixer_out(
    primary_channels: [u16; 4],
    rc_input: Option<&RcInputRawMsg>,
) -> MixerOutMsg {
    build_model_mixer_out(primary_channels, rc_input, &ModelConfig::default())
}

fn build_model_mixer_out(
    primary_channels: [u16; 4],
    rc_input: Option<&RcInputRawMsg>,
    model: &ModelConfig,
) -> MixerOutMsg {
    let mut channels = [0u16; 16];
    channels[0] = primary_channels[0];
    channels[1] = primary_channels[1];
    channels[2] = primary_channels[2];
    channels[3] = primary_channels[3];

    if let Some(input) = rc_input.filter(|input| input.switches_present) {
        for mapping in model.aux_mapping.normalized_channels() {
            let index = mapping.channel as usize - 1;
            channels[index] = aux_source_to_mixer(mapping.source, mapping.inverted, input);
        }
    }

    if channels[2] > THROTTLE_ARM_MAX {
        channels[4] = 0;
    }

    MixerOutMsg { channels }
}

fn aux_source_to_mixer(source: AuxSource, inverted: bool, input: &RcInputRawMsg) -> u16 {
    let value = match source {
        AuxSource::None => 0,
        AuxSource::Switch3Pos(index) => input
            .switch_3pos
            .get(index as usize)
            .copied()
            .map(three_pos_to_mixer)
            .unwrap_or_default(),
        AuxSource::Switch2Pos(index) => input
            .switch_2pos
            .get(index as usize)
            .copied()
            .map(two_pos_to_mixer)
            .unwrap_or_default(),
        AuxSource::Button(index) => {
            let pressed = input.buttons & (1u32 << index) != 0;
            two_pos_to_mixer(pressed)
        }
    };

    if inverted {
        10000 - value
    } else {
        value
    }
}

fn cal_mixout(channel: JoystickChannel, raw: &InputFrameMsg, cal_data: &CalibrationData) -> u16 {
    let channel_cal_info = &cal_data.channel_infos[channel as usize];

    let raw_val = raw
        .channel_value(channel_cal_info.index as usize)
        .clamp(channel_cal_info.min, channel_cal_info.max) as i32;

    let mut ret = (raw_val - channel_cal_info.min as i32) as u32 * 10000
        / (channel_cal_info.max as i32 - channel_cal_info.min as i32) as u32;

    if channel_cal_info.rev {
        ret = 10000 - ret;
    }

    ret as u16
}

fn apply_output_profile(value: u16, model: &ModelConfig, role: ControlRole) -> u16 {
    let Some(output) = model
        .mixer
        .outputs
        .iter()
        .find(|output| output.role == role)
    else {
        return value;
    };

    let centered = value as i32 - 5000;
    let weighted = centered * output.weight as i32 / 100;
    let offset = output.offset as i32 * 5;
    let mut adjusted = 5000 + weighted + offset;
    adjusted = apply_limits(adjusted, &output.limits);
    adjusted.clamp(0, 10000) as u16
}

fn apply_limits(value: i32, limits: &OutputLimits) -> i32 {
    let subtrim = limits.subtrim as i32 * 5;
    let mut adjusted = value + subtrim;
    if limits.reversed {
        adjusted = 10000 - adjusted;
    }

    let low = ((limits.min as i32 + 1000) * 5).clamp(0, 10000);
    let high = ((limits.max as i32 + 1000) * 5).clamp(0, 10000);
    adjusted.clamp(low.min(high), low.max(high))
}

fn load_calibration() -> Option<CalibrationData> {
    let mut toml_str = String::new();
    if let Ok(mut file) = fs::File::open(CALIBRATE_FILENAME) {
        file.read_to_string(&mut toml_str).unwrap();
    } else {
        thread_logln!("no joystick.toml found. please calibrate joysticks first!");
        return None;
    }

    Some(toml::from_str::<CalibrationData>(toml_str.as_str()).unwrap())
}

fn load_initial_model() -> ModelConfig {
    if let Err(err) = store::ensure_default_layout() {
        thread_logln!("config layout init failed: {}", err);
    }
    store::load_active_model().unwrap_or_default()
}

fn mixer_main(_argc: u32, _argv: *const &str) {
    let Some(cal_data) = load_calibration() else {
        return;
    };

    let rx = rpos::msg::get_new_rx_of_message::<InputFrameMsg>("input_frame").unwrap();
    let rc_input = Arc::new(Mutex::new(None::<RcInputRawMsg>));
    let tx = rpos::msg::get_new_tx_of_message::<MixerOutMsg>("mixer_out").unwrap();
    let active_model = Arc::new(Mutex::new(load_initial_model()));

    if let Some(rc_input_rx) = rpos::msg::get_new_rx_of_message::<RcInputRawMsg>("rc_input_raw") {
        let rc_input_for_updates = rc_input.clone();
        rc_input_rx.register_callback("mixer_rc_input", move |msg| {
            if let Ok(mut current) = rc_input_for_updates.lock() {
                *current = Some(*msg);
            }
        });
    }

    if let Some(active_model_rx) =
        rpos::msg::get_new_rx_of_message::<ActiveModelMsg>("active_model")
    {
        let active_model_for_updates = active_model.clone();
        active_model_rx.register_callback("mixer_active_model", move |msg| {
            if let Ok(mut current_model) = active_model_for_updates.lock() {
                *current_model = msg.model.clone();
            }
        });
    }

    rx.register_callback("mixer_callback", move |x| {
        let current_model = active_model.lock().unwrap().clone();
        let latest_rc_input = rc_input.lock().unwrap().to_owned();
        let thrust = apply_output_profile(
            cal_mixout(Thrust, x, &cal_data),
            &current_model,
            ControlRole::Thrust,
        );
        let direction = apply_output_profile(
            cal_mixout(Direction, x, &cal_data),
            &current_model,
            ControlRole::Direction,
        );
        let aileron = apply_output_profile(
            cal_mixout(Aileron, x, &cal_data),
            &current_model,
            ControlRole::Aileron,
        );
        let elevator = apply_output_profile(
            cal_mixout(Elevator, x, &cal_data),
            &current_model,
            ControlRole::Elevator,
        );
        let mut mixer_out = build_model_mixer_out(
            [aileron, elevator, thrust, direction],
            latest_rc_input.as_ref(),
            &current_model,
        );
        mixer_out.channels[4] =
            apply_output_profile(mixer_out.channels[4], &current_model, ControlRole::Arm);
        mixer_out.channels[5] = apply_output_profile(
            mixer_out.channels[5],
            &current_model,
            ControlRole::FlightMode,
        );
        mixer_out.channels[6] =
            apply_output_profile(mixer_out.channels[6], &current_model, ControlRole::Beeper);
        mixer_out.channels[7] =
            apply_output_profile(mixer_out.channels[7], &current_model, ControlRole::Turtle);
        mixer_out.channels[8] =
            apply_output_profile(mixer_out.channels[8], &current_model, ControlRole::Prearm);
        mixer_out.channels[9] = apply_output_profile(
            mixer_out.channels[9],
            &current_model,
            ControlRole::GpsRescue,
        );
        if mixer_out.channels[2] > THROTTLE_ARM_MAX {
            mixer_out.channels[4] = 0;
        }
        tx.send(mixer_out);
    });
}

#[rpos::ctor::ctor]
fn register() {
    rpos::msg::add_message::<MixerOutMsg>("mixer_out");
    rpos::module::Module::register("mixer", mixer_main);
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{AuxChannelMapping, AuxMapping, AuxSource},
        input::calibrate::ChannelInfo,
    };

    use super::*;
    use rand::prelude::*;

    #[test]
    fn test_cal_mixout() {
        let mut rng = thread_rng();
        let mut get_random_channel_value = || rng.gen_range(300..1400) as i16;
        let mut input_frame = InputFrameMsg {
            channels: vec![500, 100, 1600, get_random_channel_value()],
            ..InputFrameMsg::default()
        };
        let mut cal_data = CalibrationData {
            channel_infos: [
                ChannelInfo {
                    name: "thrust".to_string(),
                    index: 0,
                    min: 200,
                    max: 1500,
                    rev: false,
                },
                ChannelInfo {
                    name: "direction".to_string(),
                    index: 1,
                    min: 200,
                    max: 1500,
                    rev: false,
                },
                ChannelInfo {
                    name: "aliron".to_string(),
                    index: 2,
                    min: 200,
                    max: 1500,
                    rev: false,
                },
                ChannelInfo {
                    name: "ele".to_string(),
                    index: 3,
                    min: 200,
                    max: 1500,
                    rev: false,
                },
            ]
            .to_vec(),
            channel_indexs: [0; 4].to_vec(),
        };

        assert_eq!(
            cal_mixout(JoystickChannel::Thrust, &input_frame, &cal_data),
            ((500 - 200) as u32 * 10000 / (1500 - 200)) as u16
        );
        assert_eq!(
            cal_mixout(JoystickChannel::Direction, &input_frame, &cal_data),
            ((200 - 200) as u32 * 10000 / (1500 - 200)) as u16
        );
        assert_eq!(
            cal_mixout(JoystickChannel::Aileron, &input_frame, &cal_data),
            ((1500 - 200) as u32 * 10000 / (1500 - 200)) as u16
        );

        for _ in 0..1000 {
            assert!(cal_mixout(JoystickChannel::Elevator, &input_frame, &cal_data) <= 10000);
            input_frame.channels[3] = get_random_channel_value();
        }

        cal_data.channel_infos[0].rev = true;
        assert_eq!(
            cal_mixout(JoystickChannel::Thrust, &input_frame, &cal_data),
            10000 - ((500 - 200) as u32 * 10000 / (1500 - 200)) as u16
        );
    }

    #[test]
    fn test_cal_mixout_uses_zero_for_missing_channels() {
        let cal_data = CalibrationData {
            channel_infos: vec![
                ChannelInfo {
                    name: "thrust".to_string(),
                    index: 0,
                    min: 0,
                    max: 1000,
                    rev: false,
                },
                ChannelInfo {
                    name: "direction".to_string(),
                    index: 1,
                    min: 0,
                    max: 1000,
                    rev: false,
                },
                ChannelInfo {
                    name: "aileron".to_string(),
                    index: 2,
                    min: 0,
                    max: 1000,
                    rev: false,
                },
                ChannelInfo {
                    name: "elevator".to_string(),
                    index: 3,
                    min: 0,
                    max: 1000,
                    rev: false,
                },
            ],
            channel_indexs: vec![0, 1, 2, 3],
        };
        let input_frame = InputFrameMsg {
            channels: vec![300, 400],
            ..InputFrameMsg::default()
        };

        assert_eq!(
            cal_mixout(JoystickChannel::Aileron, &input_frame, &cal_data),
            0
        );
    }

    #[test]
    fn test_apply_output_profile_uses_model_settings() {
        let model = store::load_active_model().unwrap_or_else(|_| ModelConfig::default());
        let mut model = model;
        let elevator = model
            .mixer
            .outputs
            .iter_mut()
            .find(|output| output.role == ControlRole::Elevator)
            .unwrap();
        elevator.weight = 50;
        elevator.offset = 100;
        elevator.limits.reversed = true;

        let value = apply_output_profile(7000, &model, ControlRole::Elevator);
        assert!(value <= 10000);
        assert_ne!(value, 7000);
    }

    #[test]
    fn test_two_position_switch_maps_to_channel_range() {
        assert_eq!(two_pos_to_mixer(false), 0);
        assert_eq!(two_pos_to_mixer(true), 10000);
    }

    #[test]
    fn test_three_position_switch_maps_to_channel_range() {
        assert_eq!(three_pos_to_mixer(0), 0);
        assert_eq!(three_pos_to_mixer(1), 5000);
        assert_eq!(three_pos_to_mixer(2), 10000);
        assert_eq!(three_pos_to_mixer(3), 10000);
    }

    #[test]
    fn test_default_without_switch_input_keeps_arm_safe() {
        let input = rc_input_from_axes([0, 0, 0, 0]);
        let output = build_default_mixer_out([5000, 5000, 0, 5000], Some(&input));

        assert_eq!(output.channels.len(), 16);
        assert_eq!(output.channels[4], 0);
    }

    #[test]
    fn test_throttle_above_low_forces_disarm() {
        let mut input = rc_input_from_axes([0, 0, 0, 0]);
        input.switches_present = true;
        input.switch_2pos[0] = true;

        let output = build_default_mixer_out([5000, 5000, 1000, 5000], Some(&input));

        assert_eq!(output.channels[4], 0);
    }

    #[test]
    fn test_mixer_out_default_mapping_outputs_16_channels() {
        let mut input = rc_input_from_axes([0, 0, 0, 0]);
        input.switches_present = true;
        input.switch_3pos = [1, 2, 0, 1];
        input.switch_2pos = [true, true];

        let output = build_default_mixer_out([6000, 4000, 0, 7000], Some(&input));

        assert_eq!(output.channels.len(), 16);
        assert_eq!(output.channels[0], 6000);
        assert_eq!(output.channels[1], 4000);
        assert_eq!(output.channels[2], 0);
        assert_eq!(output.channels[3], 7000);
        assert_eq!(output.channels[4], 10000);
        assert_eq!(output.channels[5], 5000);
        assert_eq!(output.channels[6], 10000);
        assert_eq!(output.channels[7], 0);
        assert_eq!(output.channels[8], 5000);
        assert_eq!(output.channels[9], 10000);
        assert_eq!(output.channels[10..], [0; 6]);
    }

    #[test]
    fn test_configured_aux_mapping_can_reuse_source_across_channels() {
        let mut input = rc_input_from_axes([0, 0, 0, 0]);
        input.switches_present = true;
        input.switch_3pos[0] = 2;
        let mut model = ModelConfig::default();
        model.aux_mapping = AuxMapping {
            channels: vec![
                AuxChannelMapping {
                    channel: 5,
                    source: AuxSource::Switch3Pos(0),
                    inverted: false,
                },
                AuxChannelMapping {
                    channel: 16,
                    source: AuxSource::Switch3Pos(0),
                    inverted: false,
                },
            ],
        };

        let output = build_model_mixer_out([5000, 5000, 0, 5000], Some(&input), &model);

        assert_eq!(output.channels[4], 10000);
        assert_eq!(output.channels[15], 10000);
    }

    #[test]
    fn test_configured_aux_mapping_supports_inverted_buttons() {
        let mut input = rc_input_from_axes([0, 0, 0, 0]);
        input.switches_present = true;
        input.buttons = 1 << 3;
        let mut model = ModelConfig::default();
        model.aux_mapping = AuxMapping {
            channels: vec![AuxChannelMapping {
                channel: 12,
                source: AuxSource::Button(3),
                inverted: true,
            }],
        };

        let output = build_model_mixer_out([5000, 5000, 0, 5000], Some(&input), &model);

        assert_eq!(output.channels[11], 0);
    }
}
