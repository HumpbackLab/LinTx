use crate::{
    config::store,
    messages::{
        ElrsCommandMsg, UiFeedbackMotion, UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget,
        UiInteractionFeedback,
    },
    ui::{
        apps::{common::elrs_list_lines, AppSpec, UiAppContext, UiAppModule},
        input::UiInputEvent,
        model::{AppId, UiFrame},
    },
};

const LOCAL_POWER_LEVELS_MW: [u16; 6] = [10, 25, 100, 250, 500, 1000];

pub const SPEC: AppSpec = AppSpec {
    id: AppId::Scripts,
    title: "ELRS",
    icon_text: "ELR",
    accent: (255, 216, 109),
};

pub struct ScriptsApp;
pub static SCRIPTS_APP: ScriptsApp = ScriptsApp;

#[derive(Debug, Clone)]
struct LocalElrsConfig {
    rf_output_enabled: bool,
    wifi_manual_on: bool,
    tx_power_mw: u16,
}

impl Default for LocalElrsConfig {
    fn default() -> Self {
        Self {
            rf_output_enabled: false,
            wifi_manual_on: false,
            tx_power_mw: 100,
        }
    }
}

impl UiAppModule for ScriptsApp {
    fn on_event(&self, frame: &mut UiFrame, event: UiInputEvent, ctx: &UiAppContext<'_>) {
        if is_local_fallback(frame) {
            ensure_local_state(frame);
            if handle_local_event(frame, event, ctx.ui_feedback_tx) {
                return;
            }
        }

        match event {
            UiInputEvent::Back | UiInputEvent::PagePrev => {
                ctx.elrs_cmd_tx.send(ElrsCommandMsg::Back)
            }
            UiInputEvent::Up => ctx.elrs_cmd_tx.send(ElrsCommandMsg::SelectPrev),
            UiInputEvent::Down => ctx.elrs_cmd_tx.send(ElrsCommandMsg::SelectNext),
            UiInputEvent::Left => ctx.elrs_cmd_tx.send(ElrsCommandMsg::ValueDec),
            UiInputEvent::Right => ctx.elrs_cmd_tx.send(ElrsCommandMsg::ValueInc),
            UiInputEvent::Open => ctx.elrs_cmd_tx.send(ElrsCommandMsg::Activate),
            UiInputEvent::PageNext => ctx.elrs_cmd_tx.send(ElrsCommandMsg::Refresh),
            UiInputEvent::Quit
            | UiInputEvent::KeyboardTap { .. }
            | UiInputEvent::KeyboardSubmit => {}
        }
    }

    fn render_terminal_detail(&self, frame: &UiFrame) -> String {
        let connected = if frame.elrs.connected {
            "CONNECTED"
        } else {
            "OFFLINE"
        };
        let busy = if frame.elrs.busy { "BUSY" } else { "READY" };
        let lines = elrs_list_lines(frame);

        format!(
            "Link: {} ({})\nModule: {}\nDevice: {}\nVersion: {}\nPath: {}\nStatus: {}\n{}\n{}\n{}\n{}\n\n{}\nEsc Back",
            connected,
            busy,
            frame.elrs.module_name,
            frame.elrs.device_name,
            frame.elrs.version,
            frame.elrs.path,
            frame.elrs.status_text,
            lines[0],
            lines[1],
            lines[2],
            lines[3],
            "Up/Down: select  Left/Right: adjust  Enter: open/apply  ]: refresh",
        )
    }

    fn intercept_back(&self, frame: &UiFrame) -> bool {
        !frame.elrs.can_leave
    }

    fn on_keyboard_submit(
        &self,
        _frame: &mut UiFrame,
        _field: crate::ui::keyboard::KeyboardField,
        _value: &str,
        _ctx: &UiAppContext<'_>,
    ) -> bool {
        false
    }
}

fn is_local_fallback(frame: &UiFrame) -> bool {
    frame.elrs.path == "/" && frame.elrs.module_name == "ELRS"
}

fn ensure_local_state(frame: &mut UiFrame) {
    let cfg = load_local_config();
    apply_local_state(
        frame,
        &cfg,
        Some("Local config mode (rf_link_service offline)"),
    );
}

fn local_feedback_target(selected_idx: usize) -> UiFeedbackTarget {
    match selected_idx {
        0 => UiFeedbackTarget::FieldId("rf_output".to_string()),
        1 => UiFeedbackTarget::FieldId("wifi_manual".to_string()),
        2 => UiFeedbackTarget::FieldId("bind".to_string()),
        3 => UiFeedbackTarget::FieldId("tx_power".to_string()),
        _ => UiFeedbackTarget::SelectedListRow,
    }
}

fn set_local_feedback(
    frame: &UiFrame,
    ui_feedback_tx: &rpos::channel::Sender<UiInteractionFeedback>,
    severity: UiFeedbackSeverity,
    motion: UiFeedbackMotion,
    message: &str,
) {
    let next_seq = frame
        .interaction_feedback
        .as_ref()
        .map(|feedback| feedback.seq.wrapping_add(1))
        .unwrap_or(1);
    ui_feedback_tx.send(UiInteractionFeedback {
        seq: next_seq,
        severity,
        target: local_feedback_target(frame.elrs.selected_idx),
        motion,
        slot: UiFeedbackSlot::TopStatusBar,
        message: message.to_string(),
        ttl_ms: match severity {
            UiFeedbackSeverity::Error => 900,
            UiFeedbackSeverity::Success => 850,
            UiFeedbackSeverity::Busy => 1200,
        },
    });
}

fn handle_local_event(
    frame: &mut UiFrame,
    event: UiInputEvent,
    ui_feedback_tx: &rpos::channel::Sender<UiInteractionFeedback>,
) -> bool {
    let mut cfg = load_local_config();

    match event {
        UiInputEvent::Up => {
            frame.elrs.selected_idx = frame.elrs.selected_idx.saturating_sub(1);
            true
        }
        UiInputEvent::Down => {
            frame.elrs.selected_idx = frame.elrs.selected_idx.saturating_add(1).min(3);
            true
        }
        UiInputEvent::Left => {
            apply_local_adjust(frame, &mut cfg, -1, ui_feedback_tx);
            true
        }
        UiInputEvent::Right | UiInputEvent::Open => {
            apply_local_adjust(frame, &mut cfg, 1, ui_feedback_tx);
            true
        }
        UiInputEvent::PageNext => {
            let cfg = load_local_config();
            apply_local_state(frame, &cfg, Some("ELRS config reloaded"));
            true
        }
        UiInputEvent::Back | UiInputEvent::PagePrev => false,
        UiInputEvent::Quit
        | UiInputEvent::KeyboardTap { .. }
        | UiInputEvent::KeyboardSubmit => false,
    }
}

fn apply_local_adjust(
    frame: &mut UiFrame,
    cfg: &mut LocalElrsConfig,
    delta: isize,
    ui_feedback_tx: &rpos::channel::Sender<UiInteractionFeedback>,
) {
    let (status, severity, motion) = match frame.elrs.selected_idx {
        0 => {
            cfg.rf_output_enabled = !cfg.rf_output_enabled;
            if save_local_config(cfg).is_ok() {
                if cfg.rf_output_enabled {
                    (
                        "RF output enabled",
                        UiFeedbackSeverity::Success,
                        UiFeedbackMotion::Pulse,
                    )
                } else {
                    (
                        "RF output disabled",
                        UiFeedbackSeverity::Success,
                        UiFeedbackMotion::Pulse,
                    )
                }
            } else {
                (
                    "RF output save failed",
                    UiFeedbackSeverity::Error,
                    UiFeedbackMotion::ShakeX,
                )
            }
        }
        1 => {
            cfg.wifi_manual_on = !cfg.wifi_manual_on;
            if save_local_config(cfg).is_ok() {
                if cfg.wifi_manual_on {
                    (
                        "WiFi command armed",
                        UiFeedbackSeverity::Success,
                        UiFeedbackMotion::Pulse,
                    )
                } else {
                    (
                        "WiFi command cleared",
                        UiFeedbackSeverity::Success,
                        UiFeedbackMotion::Pulse,
                    )
                }
            } else {
                (
                    "WiFi config save failed",
                    UiFeedbackSeverity::Error,
                    UiFeedbackMotion::ShakeX,
                )
            }
        }
        2 => (
            "Bind feedback requires rf_link_service",
            UiFeedbackSeverity::Error,
            UiFeedbackMotion::ShakeX,
        ),
        3 => {
            cfg.tx_power_mw = shift_power_level(cfg.tx_power_mw, delta);
            if save_local_config(cfg).is_ok() {
                (
                    "TX power updated",
                    UiFeedbackSeverity::Success,
                    UiFeedbackMotion::Pulse,
                )
            } else {
                (
                    "TX power save failed",
                    UiFeedbackSeverity::Error,
                    UiFeedbackMotion::ShakeX,
                )
            }
        }
        _ => {
            return apply_local_state(frame, cfg, Some("ELRS"));
        }
    };

    set_local_feedback(frame, ui_feedback_tx, severity, motion, status);
    apply_local_state(frame, cfg, Some(status));
}

fn apply_local_state(frame: &mut UiFrame, cfg: &LocalElrsConfig, status: Option<&str>) {
    frame.elrs.module_name = "ELRS".to_string();
    frame.elrs.device_name = "Not Connected".to_string();
    frame.elrs.version = "--".to_string();
    frame.elrs.path = "/".to_string();
    frame.elrs.connected = false;
    frame.elrs.rf_output_enabled = cfg.rf_output_enabled;
    frame.elrs.link_active = false;
    frame.elrs.busy = false;
    frame.elrs.packet_rate = "--".to_string();
    frame.elrs.telemetry_ratio = "--".to_string();
    frame.elrs.tx_power = format!("{}mW", cfg.tx_power_mw);
    frame.elrs.wifi_running = cfg.wifi_manual_on;

    if let Some(status) = status {
        frame.elrs.status_text = status.to_string();
    }

    frame.elrs.params = vec![
        crate::messages::ElrsParamEntry {
            id: "rf_output".to_string(),
            label: "RF Output".to_string(),
            value: if cfg.rf_output_enabled {
                "ON".to_string()
            } else {
                "OFF".to_string()
            },
            selectable: true,
        },
        crate::messages::ElrsParamEntry {
            id: "wifi_manual".to_string(),
            label: "Module WiFi".to_string(),
            value: if cfg.wifi_manual_on {
                "ON".to_string()
            } else {
                "OFF".to_string()
            },
            selectable: true,
        },
        crate::messages::ElrsParamEntry {
            id: "bind".to_string(),
            label: "Bind".to_string(),
            value: "SERVICE".to_string(),
            selectable: true,
        },
        crate::messages::ElrsParamEntry {
            id: "tx_power".to_string(),
            label: "TX Power".to_string(),
            value: format!("{}mW", cfg.tx_power_mw),
            selectable: true,
        },
        crate::messages::ElrsParamEntry {
            id: "link_state".to_string(),
            label: "Link State".to_string(),
            value: if cfg.rf_output_enabled {
                "SERVICE OFFLINE".to_string()
            } else {
                "RF OFF".to_string()
            },
            selectable: false,
        },
        crate::messages::ElrsParamEntry {
            id: "signal".to_string(),
            label: "Signal".to_string(),
            value: "--".to_string(),
            selectable: false,
        },
        crate::messages::ElrsParamEntry {
            id: "aircraft_battery".to_string(),
            label: "Aircraft Battery".to_string(),
            value: "--".to_string(),
            selectable: false,
        },
        crate::messages::ElrsParamEntry {
            id: "telemetry_fresh".to_string(),
            label: "Telemetry Fresh".to_string(),
            value: "stale".to_string(),
            selectable: false,
        },
        crate::messages::ElrsParamEntry {
            id: "feedback".to_string(),
            label: "Feedback".to_string(),
            value: "start rf_link_service".to_string(),
            selectable: false,
        },
    ];
}

fn load_local_config() -> LocalElrsConfig {
    match store::load_radio_config() {
        Ok(radio) => {
            LocalElrsConfig {
                rf_output_enabled: radio.elrs.rf_output_enabled,
                wifi_manual_on: radio.elrs.wifi_manual_on,
                tx_power_mw: normalize_power_level(radio.elrs.tx_power_mw),
            }
        }
        Err(_) => LocalElrsConfig::default(),
    }
}

fn save_local_config(cfg: &LocalElrsConfig) -> Result<(), String> {
    let mut radio = store::load_radio_config().map_err(|err| err.to_string())?;
    radio.elrs.rf_output_enabled = cfg.rf_output_enabled;
    radio.elrs.wifi_manual_on = cfg.wifi_manual_on;
    radio.elrs.tx_power_mw = normalize_power_level(cfg.tx_power_mw);
    store::save_radio_config(&radio).map_err(|err| err.to_string())
}

fn shift_power_level(current: u16, delta: isize) -> u16 {
    let idx = LOCAL_POWER_LEVELS_MW
        .iter()
        .position(|power| *power == normalize_power_level(current))
        .unwrap_or(2) as isize;
    let next = (idx + delta).clamp(0, LOCAL_POWER_LEVELS_MW.len() as isize - 1) as usize;
    LOCAL_POWER_LEVELS_MW[next]
}

fn normalize_power_level(raw: u16) -> u16 {
    LOCAL_POWER_LEVELS_MW
        .iter()
        .min_by_key(|level| level.abs_diff(raw))
        .copied()
        .unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::{local_feedback_target, set_local_feedback, SCRIPTS_APP};
    use crate::{
        messages::{
            ActiveModelMsg, ElrsCommandMsg, SystemConfigMsg, UiFeedbackMotion, UiFeedbackSeverity,
            UiFeedbackSlot, UiFeedbackTarget, UsbGamepadCommandMsg,
        },
        ui::{
            apps::{UiAppContext, UiAppModule},
            input::UiInputEvent,
            model::UiFrame,
        },
    };
    use rpos::channel::Channel;

    #[test]
    fn test_local_feedback_target_for_wifi_maps_to_wifi_field() {
        assert_eq!(
            local_feedback_target(1),
            UiFeedbackTarget::FieldId("wifi_manual".to_string())
        );
    }

    #[test]
    fn test_set_local_feedback_assigns_top_status_feedback_and_increments_seq() {
        let mut frame = UiFrame::default();
        frame.elrs.selected_idx = 3;
        let (tx, mut rx) = Channel::new();

        set_local_feedback(
            &frame,
            &tx,
            UiFeedbackSeverity::Success,
            UiFeedbackMotion::Pulse,
            "TX power updated",
        );
        let first = rx.try_read().expect("first feedback");
        frame.interaction_feedback = Some(crate::ui::feedback::UiFeedbackSnapshot {
            seq: first.seq,
            severity: first.severity,
            target: first.target.clone(),
            motion: first.motion,
            slot: first.slot,
            message: first.message.clone(),
            elapsed_ms: 10,
            ttl_ms: first.ttl_ms,
        });
        set_local_feedback(
            &frame,
            &tx,
            UiFeedbackSeverity::Error,
            UiFeedbackMotion::ShakeX,
            "TX power save failed",
        );
        let second = rx.try_read().expect("second feedback");

        assert_eq!(first.slot, UiFeedbackSlot::TopStatusBar);
        assert_eq!(
            first.target,
            UiFeedbackTarget::FieldId("tx_power".to_string())
        );
        assert!(second.seq > first.seq);
    }

    #[test]
    fn test_open_on_bind_row_does_not_arm_bind_phrase_keyboard() {
        let (config_tx, _config_rx) = Channel::<SystemConfigMsg>::new();
        let (active_model_tx, _active_model_rx) = Channel::<ActiveModelMsg>::new();
        let (elrs_cmd_tx, _elrs_cmd_rx) = Channel::<ElrsCommandMsg>::new();
        let (ui_feedback_tx, mut ui_feedback_rx) = Channel::new();
        let (usb_gamepad_cmd_tx, _usb_gamepad_cmd_rx) = Channel::<UsbGamepadCommandMsg>::new();
        let ctx = UiAppContext {
            config_tx: &config_tx,
            active_model_tx: &active_model_tx,
            elrs_cmd_tx: &elrs_cmd_tx,
            ui_feedback_tx: &ui_feedback_tx,
            usb_gamepad_cmd_tx: &usb_gamepad_cmd_tx,
        };
        let mut frame = UiFrame::default();
        frame.elrs.selected_idx = 2;

        SCRIPTS_APP.on_event(&mut frame, UiInputEvent::Open, &ctx);

        assert!(frame.keyboard.is_none());
        assert_eq!(frame.keyboard_armed_field, None);
        let _ = ui_feedback_rx.try_read();
    }

    #[test]
    fn test_keyboard_submit_is_ignored() {
        let (config_tx, _config_rx) = Channel::<SystemConfigMsg>::new();
        let (active_model_tx, _active_model_rx) = Channel::<ActiveModelMsg>::new();
        let (elrs_cmd_tx, _elrs_cmd_rx) = Channel::<ElrsCommandMsg>::new();
        let (ui_feedback_tx, _ui_feedback_rx) = Channel::new();
        let (usb_gamepad_cmd_tx, _usb_gamepad_cmd_rx) = Channel::<UsbGamepadCommandMsg>::new();
        let ctx = UiAppContext {
            config_tx: &config_tx,
            active_model_tx: &active_model_tx,
            elrs_cmd_tx: &elrs_cmd_tx,
            ui_feedback_tx: &ui_feedback_tx,
            usb_gamepad_cmd_tx: &usb_gamepad_cmd_tx,
        };
        let mut frame = UiFrame::default();

        assert!(!SCRIPTS_APP.on_keyboard_submit(
            &mut frame,
            crate::ui::keyboard::KeyboardField::BindPhrase,
            "ABC",
            &ctx
        ));
    }
}
