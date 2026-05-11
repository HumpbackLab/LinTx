use crate::{
    client_process_args,
    messages::{UsbGamepadDriverCommandMsg, UsbGamepadStateMsg},
    mixer::MixerOutMsg,
};
use clap::Parser;
use rpos::{
    msg::{get_new_rx_of_message, get_new_tx_of_message},
    thread_logln,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name="usb_gamepad", about = "USB HID Gamepad output driver", long_about = None)]
struct Cli {
    /// HID device path
    #[arg(short, long, default_value = "/dev/hidg0")]
    device: String,

    /// Driver generation used to ignore stale stop commands
    #[arg(long, default_value_t = 0)]
    generation: u32,
}

/// USB HID Gamepad Report Format (7 bytes) - PS4/PS5 风格布局:
///
/// Byte 0-1: 16 buttons (bit flags, 2 bytes)
///
/// 左摇杆 (Throttle + Rudder):
///   Byte 2: X axis = Rudder/Direction (CH4 in AETR, -127~127, 回中)
///   Byte 3: Y axis = Throttle (CH3 in AETR, -127~127, 不回中)
///
/// 右摇杆 (Aileron + Elevator):  
///   Byte 4: Rx axis = Aileron/Roll (CH1 in AETR, -127~127, 回中)
///   Byte 5: Ry axis = Elevator/Pitch (CH2 in AETR, -127~127, 回中)
///
/// Byte 6: Reserved (padding)
///
/// AETR 通道顺序: CH1=Aileron, CH2=Elevator, CH3=Throttle, CH4=Rudder
#[repr(C, packed)]
struct HidGamepadReport {
    buttons_lo: u8, // Buttons 1-8
    buttons_hi: u8, // Buttons 9-16
    left_x: i8,     // 左摇杆X = Rudder/Direction (方向舵)
    left_y: i8,     // 左摇杆Y = Throttle (油门)
    right_x: i8,    // 右摇杆X = Aileron (副翼)
    right_y: i8,    // 右摇杆Y = Elevator (升降)
    _reserved: u8,  // 填充字节
}

impl HidGamepadReport {
    fn new() -> Self {
        Self {
            buttons_lo: 0,
            buttons_hi: 0,
            left_x: 0,    // Rudder 中位
            left_y: -127, // Throttle 最低 (对应 -127)
            right_x: 0,   // Aileron 中位
            right_y: 0,   // Elevator 中位
            _reserved: 0,
        }
    }

    fn to_bytes(&self) -> [u8; 7] {
        [
            self.buttons_lo,
            self.buttons_hi,
            self.left_x as u8,
            self.left_y as u8,
            self.right_x as u8,
            self.right_y as u8,
            self._reserved,
        ]
    }
}

/// Convert mixer value (0~10000) to HID axis value (-127~127)
/// Mixer 输出范围: 0 ~ 10000, 中心值 5000
/// HID 双向轴范围: -127 ~ 127, 中心值 0
fn mixer_to_hid_axis(mixer_value: u16) -> i8 {
    let normalized = (mixer_value as i32 - 5000) as f32 / 5000.0; // -1.0 ~ +1.0
    let hid_value = (normalized * 127.0) as i32;
    hid_value.clamp(-127, 127) as i8
}

/// Convert mixer throttle (0~10000) to HID axis (-127~127)
/// Mixer 油门: 0 = 最低, 10000 = 最高
/// HID 轴: -127 = 最低, 127 = 最高
fn mixer_throttle_to_hid_axis(mixer_value: u16) -> i8 {
    let normalized = (mixer_value as i32 - 5000) as f32 / 5000.0; // -1.0 ~ +1.0
    let hid_value = (normalized * 127.0) as i32;
    hid_value.clamp(-127, 127) as i8
}

fn publish_state(
    state_tx: &rpos::channel::Sender<UsbGamepadStateMsg>,
    device: &str,
    running: bool,
    report_count: u32,
    generation: u32,
    detail: impl Into<String>,
) {
    state_tx.send(UsbGamepadStateMsg {
        hid_ready: Path::new(device).exists(),
        running,
        report_count,
        generation,
        detail: detail.into(),
    });
}

fn should_publish_report_progress(report_count: u32, elapsed: Duration) -> bool {
    report_count == 1 || report_count % 100 == 0 || elapsed >= Duration::from_secs(1)
}

pub fn usb_gamepad_main(argc: u32, argv: *const &str) {
    let arg_ret = client_process_args::<Cli>(argc, argv);
    if arg_ret.is_none() {
        return;
    }

    let args = arg_ret.unwrap();

    thread_logln!("USB Gamepad driver starting...");
    thread_logln!("  Device: {}", args.device);

    let state_tx = match get_new_tx_of_message::<UsbGamepadStateMsg>("usb_gamepad_state") {
        Some(tx) => tx,
        None => {
            thread_logln!("Failed to publish usb_gamepad_state");
            return;
        }
    };
    let mut cmd_rx =
        match get_new_rx_of_message::<UsbGamepadDriverCommandMsg>("usb_gamepad_driver_cmd") {
            Some(rx) => rx,
            None => {
                thread_logln!("Failed to subscribe usb_gamepad_driver_cmd");
                return;
            }
        };

    // 订阅 mixer 输出消息
    let mut mixer_rx = match get_new_rx_of_message::<MixerOutMsg>("mixer_out") {
        Some(rx) => rx,
        None => {
            thread_logln!("Failed to subscribe to mixer_out");
            publish_state(
                &state_tx,
                &args.device,
                false,
                0,
                args.generation,
                "mixer_out unavailable",
            );
            return;
        }
    };

    // 打开 HID 设备
    let mut hid_device = match OpenOptions::new().write(true).open(&args.device) {
        Ok(f) => f,
        Err(e) => {
            thread_logln!("Failed to open HID device {}: {}", args.device, e);
            thread_logln!("Please run scripts/board/usb_gamepad/setup_hid_gamepad.sh first!");
            publish_state(
                &state_tx,
                &args.device,
                false,
                0,
                args.generation,
                format!("open {} failed: {}", args.device, e),
            );
            return;
        }
    };

    thread_logln!("USB Gamepad ready, waiting for mixer data...");
    publish_state(
        &state_tx,
        &args.device,
        true,
        0,
        args.generation,
        "USB gamepad running",
    );

    let mut counter = 0u32;
    let mut last_state_publish = Instant::now();
    loop {
        while let Some(cmd) = cmd_rx.try_read() {
            if cmd.stop && cmd.generation == args.generation {
                thread_logln!("USB Gamepad stop requested");
                publish_state(
                    &state_tx,
                    &args.device,
                    false,
                    counter,
                    args.generation,
                    "USB gamepad stopped",
                );
                return;
            }
        }

        let Some(msg) = mixer_rx.try_read() else {
            if last_state_publish.elapsed() >= Duration::from_secs(1) {
                publish_state(
                    &state_tx,
                    &args.device,
                    true,
                    counter,
                    args.generation,
                    "USB gamepad running",
                );
                last_state_publish = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };

        counter += 1;
        if counter == 1 {
            thread_logln!(
                "✓ Received first mixer data! thrust={}, dir={}, ail={}, elev={}",
                msg.channels[2],
                msg.channels[3],
                msg.channels[0],
                msg.channels[1]
            );
        }

        let mut report = HidGamepadReport::new();

        // PS4/PS5 风格 AETR 映射:
        // 左摇杆: X=Rudder/Direction, Y=Throttle
        // 右摇杆: X=Aileron, Y=Elevator
        //
        // MixerOutMsg 字段对应航模通道:
        //   direction = Rudder (CH4 in AETR)
        //   thrust    = Throttle (CH3 in AETR)
        //   aileron   = Aileron/Roll (CH1 in AETR)
        //   elevator  = Elevator/Pitch (CH2 in AETR)

        report.left_x = mixer_to_hid_axis(msg.channels[3]); // 左摇杆X = Rudder
        report.left_y = mixer_throttle_to_hid_axis(msg.channels[2]); // 左摇杆Y = Throttle
        report.right_x = mixer_to_hid_axis(msg.channels[0]); // 右摇杆X = Aileron
        report.right_y = mixer_to_hid_axis(msg.channels[1]); // 右摇杆Y = Elevator
        report._reserved = 0; // 填充字节

        // 暂时没有按键数据，保持为0
        report.buttons_lo = 0; // Buttons 1-8
        report.buttons_hi = 0; // Buttons 9-16

        // 发送 HID 报告
        let report_bytes = report.to_bytes();
        if let Err(e) = hid_device.write_all(&report_bytes) {
            thread_logln!("Failed to write HID report: {}", e);
            publish_state(
                &state_tx,
                &args.device,
                false,
                counter,
                args.generation,
                format!("write {} failed: {}", args.device, e),
            );
            return;
        }

        if should_publish_report_progress(counter, last_state_publish.elapsed()) {
            publish_state(
                &state_tx,
                &args.device,
                true,
                counter,
                args.generation,
                format!("sent {} HID reports", counter),
            );
            last_state_publish = Instant::now();
        }

        // 打印调试信息（每100次打印一次）
        if counter % 100 == 0 {
            thread_logln!(
                "HID[{}]: LX={} LY={} RX={} RY={}",
                counter,
                report.left_x,
                report.left_y,
                report.right_x,
                report.right_y
            );
        }
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("usb_gamepad", usb_gamepad_main);
}

#[cfg(test)]
mod tests {
    use super::should_publish_report_progress;
    use std::time::Duration;

    #[test]
    fn test_publish_report_progress_on_first_report() {
        assert!(should_publish_report_progress(1, Duration::from_millis(10)));
    }

    #[test]
    fn test_publish_report_progress_periodically_for_low_rate_streams() {
        assert!(should_publish_report_progress(42, Duration::from_secs(1)));
        assert!(!should_publish_report_progress(
            42,
            Duration::from_millis(500)
        ));
    }

    #[test]
    fn test_publish_report_progress_on_report_milestones() {
        assert!(should_publish_report_progress(100, Duration::from_millis(10)));
    }
}
