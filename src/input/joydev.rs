use std::collections::HashMap;

use clap::Parser;
use joydev::{event_codes::AbsoluteAxis, GenericEvent};
use rpos::thread_logln;

use crate::{
    client_process_args,
    messages::{
        publish_input_frame, publish_input_status, AdcRawMsg, InputFrameMsg, InputHealth,
        InputSource, InputStatusMsg,
    },
};

#[derive(Parser)]
#[command(name="joy_dev", about = "used for machine with joysticks(/dev/input/js*)", long_about = None)]
struct Cli {
    dev_name: String,
}

fn joy_dev_main(argc: u32, argv: *const &str) {
    let ret = client_process_args::<Cli>(argc, argv);
    if ret.is_none() {
        return;
    }

    let args = ret.unwrap();
    let adc_raw_tx = rpos::msg::get_new_tx_of_message::<AdcRawMsg>("adc_raw").unwrap();
    let input_frame_tx = rpos::msg::get_new_tx_of_message::<InputFrameMsg>("input_frame").unwrap();
    let input_status_tx =
        rpos::msg::get_new_tx_of_message::<InputStatusMsg>("input_status").unwrap();
    let file = match std::fs::File::options().read(true).open(&args.dev_name) {
        Ok(file) => file,
        Err(err) => {
            publish_input_status(
                &input_status_tx,
                InputSource::JoyDev,
                InputHealth::Error,
                format!("open {} failed: {}", args.dev_name, err),
                4,
            );
            thread_logln!("joy_dev open failed on {}: {}", args.dev_name, err);
            return;
        }
    };
    let dev = match joydev::Device::new(file) {
        Ok(dev) => dev,
        Err(err) => {
            publish_input_status(
                &input_status_tx,
                InputSource::JoyDev,
                InputHealth::Error,
                format!("init {} failed: {}", args.dev_name, err),
                4,
            );
            thread_logln!("joy_dev init failed on {}: {}", args.dev_name, err);
            return;
        }
    };
    let chn_map: HashMap<AbsoluteAxis, usize> = [
        (AbsoluteAxis::LeftX, 0),
        (AbsoluteAxis::LeftY, 1),
        (AbsoluteAxis::RightX, 2),
        (AbsoluteAxis::RightY, 3),
    ]
    .into_iter()
    .collect();
    let mut chn_value: [i16; 4] = [0; 4];
    publish_input_status(
        &input_status_tx,
        InputSource::JoyDev,
        InputHealth::Running,
        args.dev_name.clone(),
        4,
    );

    loop {
        let s = dev.get_event().unwrap();
        match s {
            joydev::DeviceEvent::Axis(x) => {
                if let Some(index) = chn_map.get(&x.axis()) {
                    chn_value[*index] = x.value();
                    publish_input_frame(
                        &input_frame_tx,
                        Some(&adc_raw_tx),
                        InputSource::JoyDev,
                        &chn_value,
                    );
                }
            }
            _ => {}
        }
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("joy_dev", joy_dev_main);
}
