use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use rpos::msg::get_new_tx_of_message;

use crate::{
    client_process_args,
    messages::{SystemConfigMsg, SystemStatusMsg},
};

#[derive(Parser)]
#[command(name = "system_state_mock", about = "Mock system status/config producer", long_about = None)]
struct Cli {
    #[arg(long, default_value_t = 5)]
    hz: u32,
}

fn system_state_mock_main(argc: u32, argv: *const &str) {
    let args = match client_process_args::<Cli>(argc, argv) {
        Some(a) => a,
        None => return,
    };

    let status_tx = get_new_tx_of_message::<SystemStatusMsg>("system_status").unwrap();
    let config_tx = get_new_tx_of_message::<SystemConfigMsg>("system_config").unwrap();

    let interval = Duration::from_millis((1000 / args.hz.max(1)) as u64);
    let mut tick: u64 = 0;

    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs();

        let status = SystemStatusMsg {
            remote_battery_percent: (100_u64.saturating_sub((tick / 10) % 100)) as u8,
            aircraft_battery_percent: (95_u64.saturating_sub((tick / 15) % 90)) as u8,
            signal_strength_percent: (60 + (tick % 40) as u8).min(100),
            unix_time_secs: now,
        };

        let config = SystemConfigMsg {
            backlight_percent: (40 + (tick % 50) as u8).min(100),
            sound_percent: (30 + ((tick * 3) % 60) as u8).min(100),
        };

        status_tx.send(status);
        config_tx.send(config);

        tick = tick.wrapping_add(1);
        std::thread::sleep(interval);
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("system_state_mock", system_state_mock_main);
}
