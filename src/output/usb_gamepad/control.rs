use crate::messages::{
    UiFeedbackMotion, UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget, UiInteractionFeedback,
    UsbGamepadCommandMsg, UsbGamepadDriverCommandMsg, UsbGamepadStateMsg,
};
use rpos::{
    module::Module,
    msg::{get_new_rx_of_message, get_new_tx_of_message},
    pthread_scheduler::SchedulePthread,
    thread_logln,
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

const HID_DEVICE_PATH: &str = "/dev/hidg0";
const REPORT_STALL_RETRY_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartDecision {
    Spawn,
    Stop,
    AlreadyRunning,
    AlreadyStopped,
    MissingDevice,
    MissingModule,
}

fn should_auto_start(auto_pending: bool, hid_ready: bool, running: bool) -> bool {
    auto_pending && hid_ready && !running
}

fn should_retry_report_stall(
    hid_ready: bool,
    running: bool,
    report_count: u32,
    last_report_count: u32,
    stalled_for: Duration,
) -> bool {
    hid_ready
        && running
        && report_count == last_report_count
        && stalled_for >= REPORT_STALL_RETRY_AFTER
}

fn decide_start(
    device_exists: bool,
    running: bool,
    module_available: bool,
    command: UsbGamepadCommandMsg,
) -> StartDecision {
    match command {
        UsbGamepadCommandMsg::Stop => {
            if running {
                StartDecision::Stop
            } else {
                StartDecision::AlreadyStopped
            }
        }
        UsbGamepadCommandMsg::Start => {
            if !device_exists {
                StartDecision::MissingDevice
            } else if running {
                StartDecision::AlreadyRunning
            } else if !module_available {
                StartDecision::MissingModule
            } else {
                StartDecision::Spawn
            }
        }
        UsbGamepadCommandMsg::Toggle => {
            if running {
                StartDecision::Stop
            } else if !device_exists {
                StartDecision::MissingDevice
            } else if !module_available {
                StartDecision::MissingModule
            } else {
                StartDecision::Spawn
            }
        }
    }
}

fn feedback(
    seq: u32,
    severity: UiFeedbackSeverity,
    motion: UiFeedbackMotion,
    message: impl Into<String>,
) -> UiInteractionFeedback {
    UiInteractionFeedback {
        seq,
        severity,
        target: UiFeedbackTarget::Page,
        motion,
        slot: UiFeedbackSlot::TopStatusBar,
        message: message.into(),
        ttl_ms: 1600,
    }
}

fn spawn_usb_gamepad(
    state_tx: &rpos::channel::Sender<UsbGamepadStateMsg>,
    ui_feedback_tx: &rpos::channel::Sender<UiInteractionFeedback>,
    seq: u32,
    next_generation: &mut u32,
    detail: &'static str,
    feedback_message: &'static str,
) {
    let Some(module) = Module::try_get_module("usb_gamepad") else {
        ui_feedback_tx.send(feedback(
            seq,
            UiFeedbackSeverity::Error,
            UiFeedbackMotion::ShakeX,
            "usb_gamepad module unavailable",
        ));
        return;
    };
    let generation = *next_generation;
    *next_generation = next_generation.wrapping_add(1).max(1);
    let worker = SchedulePthread::new_simple(Box::new(move |_| {
        let generation_arg = generation.to_string();
        let argv_owned = [
            "usb_gamepad".to_string(),
            "--device".to_string(),
            HID_DEVICE_PATH.to_string(),
            "--generation".to_string(),
            generation_arg,
        ];
        let argv: Vec<&str> = argv_owned.iter().map(|arg| arg.as_str()).collect();
        module.execute(argv.len() as u32, argv.as_ptr());
    }));
    std::mem::forget(worker);
    state_tx.send(UsbGamepadStateMsg {
        hid_ready: true,
        running: true,
        report_count: 0,
        generation,
        detail: detail.to_string(),
    });
    ui_feedback_tx.send(feedback(
        seq,
        UiFeedbackSeverity::Success,
        UiFeedbackMotion::Pulse,
        feedback_message,
    ));
}

fn handle_usb_gamepad_command(
    driver_command_tx: &rpos::channel::Sender<UsbGamepadDriverCommandMsg>,
    state_tx: &rpos::channel::Sender<UsbGamepadStateMsg>,
    ui_feedback_tx: &rpos::channel::Sender<UiInteractionFeedback>,
    seq: &mut u32,
    state: &UsbGamepadStateMsg,
    command: UsbGamepadCommandMsg,
    next_generation: &mut u32,
) {
    *seq = seq.wrapping_add(1);
    let module = Module::try_get_module("usb_gamepad");
    let decision = decide_start(
        Path::new(HID_DEVICE_PATH).exists(),
        state.running,
        module.is_some(),
        command,
    );

    match decision {
        StartDecision::MissingDevice => {
            ui_feedback_tx.send(feedback(
                *seq,
                UiFeedbackSeverity::Error,
                UiFeedbackMotion::ShakeX,
                "USB HID not ready: /dev/hidg0 missing",
            ));
        }
        StartDecision::AlreadyRunning => {
            ui_feedback_tx.send(feedback(
                *seq,
                UiFeedbackSeverity::Success,
                UiFeedbackMotion::Pulse,
                "USB gamepad already running",
            ));
        }
        StartDecision::AlreadyStopped => {
            ui_feedback_tx.send(feedback(
                *seq,
                UiFeedbackSeverity::Success,
                UiFeedbackMotion::Pulse,
                "USB gamepad already off",
            ));
        }
        StartDecision::MissingModule => {
            ui_feedback_tx.send(feedback(
                *seq,
                UiFeedbackSeverity::Error,
                UiFeedbackMotion::ShakeX,
                "usb_gamepad module unavailable",
            ));
        }
        StartDecision::Stop => {
            driver_command_tx.send(UsbGamepadDriverCommandMsg {
                generation: state.generation,
                stop: true,
            });
            state_tx.send(UsbGamepadStateMsg {
                hid_ready: Path::new(HID_DEVICE_PATH).exists(),
                running: false,
                report_count: state.report_count,
                generation: state.generation,
                detail: "Stopping USB gamepad".to_string(),
            });
            ui_feedback_tx.send(feedback(
                *seq,
                UiFeedbackSeverity::Success,
                UiFeedbackMotion::Pulse,
                "USB gamepad stopping",
            ));
        }
        StartDecision::Spawn => {
            spawn_usb_gamepad(
                state_tx,
                ui_feedback_tx,
                *seq,
                next_generation,
                "Starting USB gamepad",
                "USB gamepad started",
            );
        }
    }
}

fn usb_gamepad_control_main(_argc: u32, _argv: *const &str) {
    let mut cmd_rx = match get_new_rx_of_message::<UsbGamepadCommandMsg>("usb_gamepad_cmd") {
        Some(rx) => rx,
        None => {
            thread_logln!("usb_gamepad_control failed to subscribe usb_gamepad_cmd");
            return;
        }
    };
    let ui_feedback_tx =
        match get_new_tx_of_message::<UiInteractionFeedback>("ui_interaction_feedback") {
            Some(tx) => tx,
            None => {
                thread_logln!("usb_gamepad_control failed to publish ui feedback");
                return;
            }
        };
    let driver_command_tx =
        match get_new_tx_of_message::<UsbGamepadDriverCommandMsg>("usb_gamepad_driver_cmd") {
            Some(tx) => tx,
            None => {
                thread_logln!("usb_gamepad_control failed to publish usb_gamepad_driver_cmd");
                return;
            }
        };
    let state_tx = match get_new_tx_of_message::<UsbGamepadStateMsg>("usb_gamepad_state") {
        Some(tx) => tx,
        None => {
            thread_logln!("usb_gamepad_control failed to publish usb_gamepad_state");
            return;
        }
    };
    let mut state_rx = match get_new_rx_of_message::<UsbGamepadStateMsg>("usb_gamepad_state") {
        Some(rx) => rx,
        None => {
            thread_logln!("usb_gamepad_control failed to subscribe usb_gamepad_state");
            return;
        }
    };

    thread_logln!("usb_gamepad_control ready");
    let mut seq = 0x7600;
    let mut state = UsbGamepadStateMsg {
        hid_ready: Path::new(HID_DEVICE_PATH).exists(),
        running: false,
        report_count: 0,
        generation: 0,
        detail: if Path::new(HID_DEVICE_PATH).exists() {
            "USB gamepad off".to_string()
        } else {
            "Waiting for /dev/hidg0".to_string()
        },
    };
    state_tx.send(state.clone());
    let mut auto_start_pending = true;
    let mut next_generation = 1;
    let mut last_report_count = state.report_count;
    let mut last_report_change = Instant::now();
    loop {
        while let Some(next_state) = state_rx.try_read() {
            state = next_state;
            if !state.running || state.report_count != last_report_count {
                last_report_count = state.report_count;
                last_report_change = Instant::now();
            }
        }
        while let Some(command) = cmd_rx.try_read() {
            auto_start_pending = false;
            handle_usb_gamepad_command(
                &driver_command_tx,
                &state_tx,
                &ui_feedback_tx,
                &mut seq,
                &state,
                command,
                &mut next_generation,
            );
        }
        let hid_ready = Path::new(HID_DEVICE_PATH).exists();
        if state.hid_ready != hid_ready {
            state.hid_ready = hid_ready;
            if !hid_ready {
                state.running = false;
                state.detail = "Waiting for /dev/hidg0".to_string();
            }
            state_tx.send(state.clone());
        }
        if should_auto_start(auto_start_pending, state.hid_ready, state.running) {
            handle_usb_gamepad_command(
                &driver_command_tx,
                &state_tx,
                &ui_feedback_tx,
                &mut seq,
                &state,
                UsbGamepadCommandMsg::Start,
                &mut next_generation,
            );
            auto_start_pending = false;
        }
        if should_retry_report_stall(
            state.hid_ready,
            state.running,
            state.report_count,
            last_report_count,
            last_report_change.elapsed(),
        ) {
            seq = seq.wrapping_add(1);
            driver_command_tx.send(UsbGamepadDriverCommandMsg {
                generation: state.generation,
                stop: true,
            });
            state_tx.send(UsbGamepadStateMsg {
                hid_ready: state.hid_ready,
                running: false,
                report_count: state.report_count,
                generation: state.generation,
                detail: "Retrying stalled USB gamepad".to_string(),
            });
            spawn_usb_gamepad(
                &state_tx,
                &ui_feedback_tx,
                seq,
                &mut next_generation,
                "Retrying stalled USB gamepad",
                "USB gamepad retrying",
            );
            last_report_change = Instant::now();
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[rpos::ctor::ctor]
fn register() {
    Module::register("usb_gamepad_control", usb_gamepad_control_main);
}

#[cfg(test)]
mod tests {
    use super::{decide_start, should_auto_start, should_retry_report_stall, StartDecision};
    use crate::messages::UsbGamepadCommandMsg;
    use std::time::Duration;

    #[test]
    fn test_decide_start_reports_missing_hid_before_any_spawn() {
        assert_eq!(
            decide_start(false, false, true, UsbGamepadCommandMsg::Start),
            StartDecision::MissingDevice
        );
    }

    #[test]
    fn test_decide_start_spawns_only_when_device_and_module_are_ready() {
        assert_eq!(
            decide_start(true, false, true, UsbGamepadCommandMsg::Start),
            StartDecision::Spawn
        );
        assert_eq!(
            decide_start(true, true, true, UsbGamepadCommandMsg::Start),
            StartDecision::AlreadyRunning
        );
        assert_eq!(
            decide_start(true, false, false, UsbGamepadCommandMsg::Start),
            StartDecision::MissingModule
        );
    }

    #[test]
    fn test_decide_start_toggles_running_gamepad_off() {
        assert_eq!(
            decide_start(true, true, true, UsbGamepadCommandMsg::Toggle),
            StartDecision::Stop
        );
        assert_eq!(
            decide_start(true, false, true, UsbGamepadCommandMsg::Stop),
            StartDecision::AlreadyStopped
        );
    }

    #[test]
    fn test_auto_start_waits_for_hid_and_only_runs_once() {
        assert!(!should_auto_start(true, false, false));
        assert!(should_auto_start(true, true, false));
        assert!(!should_auto_start(true, true, true));
        assert!(!should_auto_start(false, true, false));
    }

    #[test]
    fn test_retry_stalled_reports_only_when_running_and_count_is_unchanged() {
        assert!(should_retry_report_stall(
            true,
            true,
            10,
            10,
            super::REPORT_STALL_RETRY_AFTER
        ));
        assert!(!should_retry_report_stall(
            true,
            true,
            11,
            10,
            super::REPORT_STALL_RETRY_AFTER
        ));
        assert!(!should_retry_report_stall(
            false,
            true,
            10,
            10,
            super::REPORT_STALL_RETRY_AFTER
        ));
        assert!(!should_retry_report_stall(
            true,
            false,
            10,
            10,
            super::REPORT_STALL_RETRY_AFTER
        ));
        assert!(!should_retry_report_stall(
            true,
            true,
            10,
            10,
            Duration::from_millis(500)
        ));
    }
}
