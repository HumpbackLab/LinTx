use crate::{
    client_process_args,
    messages::{
        publish_input_frame, publish_input_status, AdcRawMsg, InputFrameMsg, InputHealth,
        InputSource, InputStatusMsg,
    },
};
use clap::Parser;
use crsf::{PacketParser, RcChannels};
use rpos::{msg::get_new_tx_of_message, thread_logln};
use std::time::Duration;

#[derive(Parser)]
#[command(
    name="crsf_rc_in",
    about = "Read RC data from an external CRSF source",
    long_about = None
)]
struct Cli {
    #[arg(short, long, default_value_t = 420000)]
    // CRSF typically uses 420k, but can be 115200. Defaulting to standard CRSF.
    baudrate: u32,

    dev_name: String,
}

fn handle_channels(
    channels: &RcChannels,
    frame_tx: &rpos::channel::Sender<InputFrameMsg>,
    legacy_adc_tx: &rpos::channel::Sender<AdcRawMsg>,
) {
    let mut mapped_values = channels
        .0
        .iter()
        .map(|channel| *channel as i16)
        .collect::<Vec<_>>();
    if mapped_values.len() >= 4 {
        let aileron = mapped_values[0];
        let elevator = mapped_values[1];
        let thrust = mapped_values[2];
        let direction = mapped_values[3];
        mapped_values[0] = thrust;
        mapped_values[1] = direction;
        mapped_values[2] = aileron;
        mapped_values[3] = elevator;
    }

    publish_input_frame(
        frame_tx,
        Some(legacy_adc_tx),
        InputSource::CrsfRcIn,
        &mapped_values,
    );
}

pub fn crsf_rc_in_main(argc: u32, argv: *const &str) {
    let arg_ret = client_process_args::<Cli>(argc, argv);
    if arg_ret.is_none() {
        return;
    }

    let args = arg_ret.unwrap();

    let serial = serialport::new(&args.dev_name, args.baudrate);
    match serial.timeout(Duration::from_millis(100)).open() {
        Ok(mut port) => {
            thread_logln!(
                "crsf_rc_in start on {} @ {} baud",
                args.dev_name,
                args.baudrate
            );

            let adc_raw_tx = get_new_tx_of_message::<AdcRawMsg>("adc_raw").unwrap();
            let input_frame_tx = get_new_tx_of_message::<InputFrameMsg>("input_frame").unwrap();
            let input_status_tx = get_new_tx_of_message::<InputStatusMsg>("input_status").unwrap();
            let mut parser = PacketParser::<1024>::new(); // Internal buffer size
            let mut buf = [0u8; 1024];
            publish_input_status(
                &input_status_tx,
                InputSource::CrsfRcIn,
                InputHealth::Running,
                format!("{} @ {} baud", args.dev_name, args.baudrate),
                16,
            );

            loop {
                match port.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        parser.push_bytes(&buf[..n]);
                        while let Some(packet) = parser.next_packet() {
                            match packet {
                                Ok((_addr, crsf::Packet::RcChannels(channels))) => {
                                    handle_channels(&channels, &input_frame_tx, &adc_raw_tx);
                                }
                                Ok(_) => {
                                    // Ignore telemetry or other packets for now
                                }
                                Err(_) => {}
                            }
                        }
                    }
                    Ok(_) => {
                        // EOF or no data, sleep a bit
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        // Timeout is fine, just continue
                    }
                    Err(e) => {
                        thread_logln!("Serial read error: {}", e);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }
        Err(e) => {
            if let Some(input_status_tx) = get_new_tx_of_message::<InputStatusMsg>("input_status") {
                publish_input_status(
                    &input_status_tx,
                    InputSource::CrsfRcIn,
                    InputHealth::Error,
                    format!("open {} failed: {}", args.dev_name, e),
                    16,
                );
            }
            thread_logln!("Failed to open serial port {}: {}", args.dev_name, e);
        }
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("crsf_rc_in", crsf_rc_in_main);
}
