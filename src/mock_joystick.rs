use crate::{
    client_process_args,
    messages::{
        publish_input_frame, publish_input_status, publish_rc_input_raw, AdcRawMsg, InputFrameMsg,
        InputHealth, InputSource, InputStatusMsg, RcInputRawMsg,
    },
};
use clap::Parser;
use rpos::{msg::get_new_tx_of_message, thread_logln};
use serde::Deserialize;
use std::f32::consts::PI;
use std::time::Duration;

#[derive(Parser)]
#[command(name="mock_joystick", about = "Mock joystick data generator for testing", long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "mock_config.toml")]
    config: String,
}

#[derive(Debug, Deserialize)]
struct MockConfig {
    #[serde(default = "default_mode")]
    mode: String,

    #[serde(default = "default_update_rate")]
    update_rate_hz: u32,

    #[serde(default)]
    static_config: StaticConfig,

    #[serde(default)]
    sine_config: SineConfig,

    #[serde(default)]
    step_config: StepConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct StaticConfig {
    #[serde(default = "default_static_channels")]
    channels: Vec<i16>,

    #[serde(default)]
    switch_3pos: [u8; 4],

    #[serde(default)]
    switch_2pos: [bool; 2],

    #[serde(default)]
    buttons: u32,
}

#[derive(Debug, Deserialize, Clone)]
struct SineConfig {
    #[serde(default = "default_sine_base")]
    base: Vec<i16>,

    #[serde(default = "default_sine_amplitude")]
    amplitude: Vec<i16>,

    #[serde(default = "default_sine_frequency")]
    frequency_hz: Vec<f32>,
}

#[derive(Debug, Deserialize, Clone)]
struct StepConfig {
    #[serde(default = "default_step_values")]
    values: Vec<Vec<i16>>,

    #[serde(default = "default_step_duration")]
    step_duration_ms: u64,
}

// Default value functions
fn default_mode() -> String {
    "static".to_string()
}
fn default_update_rate() -> u32 {
    50
}
fn default_static_channels() -> Vec<i16> {
    vec![992, 992, 0, 992]
}
fn default_sine_base() -> Vec<i16> {
    vec![992, 992, 0, 992]
}
fn default_sine_amplitude() -> Vec<i16> {
    vec![100, 100, 0, 100]
}
fn default_sine_frequency() -> Vec<f32> {
    vec![1.0, 0.5, 0.0, 2.0]
}
fn default_step_values() -> Vec<Vec<i16>> {
    vec![
        vec![0, 0, 0, 0],
        vec![992, 992, 0, 992],
        vec![1984, 1984, 1984, 1984],
    ]
}
fn default_step_duration() -> u64 {
    1000
}

impl Default for StaticConfig {
    fn default() -> Self {
        Self {
            channels: default_static_channels(),
            switch_3pos: [0; 4],
            switch_2pos: [false; 2],
            buttons: 0,
        }
    }
}

impl Default for SineConfig {
    fn default() -> Self {
        Self {
            base: default_sine_base(),
            amplitude: default_sine_amplitude(),
            frequency_hz: default_sine_frequency(),
        }
    }
}

impl Default for StepConfig {
    fn default() -> Self {
        Self {
            values: default_step_values(),
            step_duration_ms: default_step_duration(),
        }
    }
}

pub fn mock_joystick_main(argc: u32, argv: *const &str) {
    let arg_ret = client_process_args::<Cli>(argc, argv);
    if arg_ret.is_none() {
        return;
    }

    let args = arg_ret.unwrap();

    // Load configuration
    let config = match std::fs::read_to_string(&args.config) {
        Ok(content) => match toml::from_str::<MockConfig>(&content) {
            Ok(cfg) => {
                thread_logln!("Loaded mock config from: {}", args.config);
                cfg
            }
            Err(e) => {
                thread_logln!("Failed to parse config file: {}, using defaults", e);
                MockConfig {
                    mode: default_mode(),
                    update_rate_hz: default_update_rate(),
                    static_config: StaticConfig::default(),
                    sine_config: SineConfig::default(),
                    step_config: StepConfig::default(),
                }
            }
        },
        Err(_) => {
            thread_logln!("Config file not found: {}, using defaults", args.config);
            MockConfig {
                mode: default_mode(),
                update_rate_hz: default_update_rate(),
                static_config: StaticConfig::default(),
                sine_config: SineConfig::default(),
                step_config: StepConfig::default(),
            }
        }
    };

    let adc_raw_tx = get_new_tx_of_message::<AdcRawMsg>("adc_raw").unwrap();
    let input_frame_tx = get_new_tx_of_message::<InputFrameMsg>("input_frame").unwrap();
    let rc_input_raw_tx = get_new_tx_of_message::<RcInputRawMsg>("rc_input_raw").unwrap();
    let input_status_tx = get_new_tx_of_message::<InputStatusMsg>("input_status").unwrap();
    let update_interval = Duration::from_millis(1000 / config.update_rate_hz as u64);

    thread_logln!(
        "Mock joystick started in '{}' mode @ {} Hz",
        config.mode,
        config.update_rate_hz
    );
    let channel_count = match config.mode.as_str() {
        "static" => config.static_config.channels.len(),
        "sine" => config
            .sine_config
            .base
            .len()
            .max(config.sine_config.amplitude.len())
            .max(config.sine_config.frequency_hz.len()),
        "step" => config
            .step_config
            .values
            .iter()
            .map(|channels| channels.len())
            .max()
            .unwrap_or(0),
        _ => config.static_config.channels.len(),
    };
    publish_input_status(
        &input_status_tx,
        InputSource::Mock,
        InputHealth::Running,
        format!("mode={} @ {}Hz", config.mode, config.update_rate_hz),
        channel_count,
    );

    match config.mode.as_str() {
        "static" => run_static_mode(
            &config.static_config,
            &input_frame_tx,
            &adc_raw_tx,
            &rc_input_raw_tx,
            update_interval,
        ),
        "sine" => run_sine_mode(
            &config.sine_config,
            &input_frame_tx,
            &adc_raw_tx,
            update_interval,
        ),
        "step" => run_step_mode(
            &config.step_config,
            &input_frame_tx,
            &adc_raw_tx,
            update_interval,
        ),
        _ => {
            thread_logln!("Unknown mode: {}, falling back to static", config.mode);
            run_static_mode(
                &config.static_config,
                &input_frame_tx,
                &adc_raw_tx,
                &rc_input_raw_tx,
                update_interval,
            );
        }
    }
}

fn run_static_mode(
    config: &StaticConfig,
    frame_tx: &rpos::channel::Sender<InputFrameMsg>,
    legacy_adc_tx: &rpos::channel::Sender<AdcRawMsg>,
    rc_input_raw_tx: &rpos::channel::Sender<RcInputRawMsg>,
    interval: Duration,
) {
    let channels = config.channels.clone();
    let axes = vec_to_axes(&channels);
    let input = RcInputRawMsg {
        axes,
        switch_3pos: config.switch_3pos,
        switch_2pos: config.switch_2pos,
        buttons: config.buttons,
        switches_present: true,
    };
    thread_logln!(
        "Static mode: channels = {:?}, switch_3pos = {:?}, switch_2pos = {:?}",
        channels,
        input.switch_3pos,
        input.switch_2pos
    );

    loop {
        publish_rc_input_raw(
            rc_input_raw_tx,
            frame_tx,
            Some(legacy_adc_tx),
            InputSource::Mock,
            input,
        );
        std::thread::sleep(interval);
    }
}

fn vec_to_axes(channels: &[i16]) -> [i16; 4] {
    let mut axes = [0i16; 4];
    for (index, value) in channels.iter().copied().enumerate().take(axes.len()) {
        axes[index] = value;
    }
    axes
}

fn run_sine_mode(
    config: &SineConfig,
    frame_tx: &rpos::channel::Sender<InputFrameMsg>,
    legacy_adc_tx: &rpos::channel::Sender<AdcRawMsg>,
    interval: Duration,
) {
    let mut time = 0.0f32;
    let dt = interval.as_secs_f32();

    thread_logln!(
        "Sine mode: base = {:?}, amplitude = {:?}, freq = {:?}",
        config.base,
        config.amplitude,
        config.frequency_hz
    );

    loop {
        let channel_count = config
            .base
            .len()
            .max(config.amplitude.len())
            .max(config.frequency_hz.len());
        let mut channels = vec![0i16; channel_count];

        for (i, channel) in channels.iter_mut().enumerate() {
            let base = config.base.get(i).copied().unwrap_or(0);
            let amplitude = config.amplitude.get(i).copied().unwrap_or(0);
            let frequency = config.frequency_hz.get(i).copied().unwrap_or(0.0);

            let sine_value = (2.0 * PI * frequency * time).sin();
            let value = base + (amplitude as f32 * sine_value) as i16;

            *channel = value.clamp(-2048, 2047);
        }

        publish_input_frame(frame_tx, Some(legacy_adc_tx), InputSource::Mock, &channels);

        time += dt;
        if time > 1000.0 {
            time -= 1000.0; // Prevent overflow
        }

        std::thread::sleep(interval);
    }
}

fn run_step_mode(
    config: &StepConfig,
    frame_tx: &rpos::channel::Sender<InputFrameMsg>,
    legacy_adc_tx: &rpos::channel::Sender<AdcRawMsg>,
    interval: Duration,
) {
    if config.values.is_empty() {
        thread_logln!("Step mode: no values configured, exiting");
        return;
    }

    thread_logln!(
        "Step mode: {} steps, {} ms per step",
        config.values.len(),
        config.step_duration_ms
    );

    let steps_per_duration =
        (config.step_duration_ms as f32 / interval.as_millis() as f32).ceil() as usize;

    let mut step_index = 0;

    loop {
        let step_values = &config.values[step_index];

        // Send the same value multiple times for the step duration
        for _ in 0..steps_per_duration {
            publish_input_frame(
                frame_tx,
                Some(legacy_adc_tx),
                InputSource::Mock,
                step_values,
            );
            std::thread::sleep(interval);
        }

        step_index = (step_index + 1) % config.values.len();
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("mock_joystick", mock_joystick_main);
}
