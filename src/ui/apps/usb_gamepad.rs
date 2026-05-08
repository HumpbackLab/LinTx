use crate::{
    messages::{
        UiFeedbackMotion, UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget,
        UiInteractionFeedback, UsbGamepadCommandMsg,
    },
    ui::{
        apps::{AppSpec, UiAppContext, UiAppModule},
        input::UiInputEvent,
        model::{AppId, UiFrame},
    },
};

pub const SPEC: AppSpec = AppSpec {
    id: AppId::UsbGamepad,
    title: "USB PAD",
    icon_text: "USB",
    accent: (88, 184, 120),
};

pub struct UsbGamepadApp;
pub static USB_GAMEPAD_APP: UsbGamepadApp = UsbGamepadApp;

fn feedback(seq: u32, message: impl Into<String>) -> UiInteractionFeedback {
    UiInteractionFeedback {
        seq,
        severity: UiFeedbackSeverity::Busy,
        target: UiFeedbackTarget::Page,
        motion: UiFeedbackMotion::Pulse,
        slot: UiFeedbackSlot::TopStatusBar,
        message: message.into(),
        ttl_ms: 1200,
    }
}

impl UiAppModule for UsbGamepadApp {
    fn on_event(&self, _frame: &mut UiFrame, event: UiInputEvent, ctx: &UiAppContext<'_>) {
        if matches!(
            event,
            UiInputEvent::Open | UiInputEvent::Left | UiInputEvent::Right
        ) {
            ctx.usb_gamepad_cmd_tx.send(UsbGamepadCommandMsg::Toggle);
            ctx.ui_feedback_tx
                .send(feedback(0x7500, "Toggling USB gamepad"));
        }
    }

    fn render_terminal_detail(&self, frame: &UiFrame) -> String {
        format!(
            "USB HID Gamepad\n> Output: {}\nDevice: /dev/hidg0 [{}]\nDetail: {}\nInput: {} ({})\nMixer: {}/{}/{}/{}\n\nEnter/Left/Right Toggle\nEsc Back",
            if frame.usb_gamepad.running { "ON" } else { "OFF" },
            if frame.usb_gamepad.hid_ready { "ready" } else { "missing" },
            frame.usb_gamepad.detail,
            frame.input_status.source.label(),
            frame.input_status.health.label(),
            frame.mixer_out.channels[0],
            frame.mixer_out.channels[1],
            frame.mixer_out.channels[2],
            frame.mixer_out.channels[3],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::USB_GAMEPAD_APP;
    use crate::{
        messages::{UiFeedbackSeverity, UiFeedbackSlot, UsbGamepadCommandMsg},
        ui::{
            apps::{UiAppContext, UiAppModule},
            input::UiInputEvent,
            model::UiFrame,
        },
    };
    use rpos::channel::Channel;

    #[test]
    fn test_open_requests_usb_gamepad_start_and_shows_busy_feedback() {
        let (config_tx, _config_rx) = Channel::new();
        let (active_model_tx, _active_model_rx) = Channel::new();
        let (elrs_cmd_tx, _elrs_cmd_rx) = Channel::new();
        let (ui_feedback_tx, mut ui_feedback_rx) = Channel::new();
        let (usb_gamepad_cmd_tx, mut usb_gamepad_cmd_rx) = Channel::new();
        let ctx = UiAppContext {
            config_tx: &config_tx,
            active_model_tx: &active_model_tx,
            elrs_cmd_tx: &elrs_cmd_tx,
            ui_feedback_tx: &ui_feedback_tx,
            usb_gamepad_cmd_tx: &usb_gamepad_cmd_tx,
        };
        let mut frame = UiFrame::default();

        USB_GAMEPAD_APP.on_event(&mut frame, UiInputEvent::Open, &ctx);

        assert_eq!(
            usb_gamepad_cmd_rx.try_read(),
            Some(UsbGamepadCommandMsg::Toggle)
        );
        let feedback = ui_feedback_rx.try_read().expect("ui feedback");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Busy);
        assert_eq!(feedback.slot, UiFeedbackSlot::TopStatusBar);
        assert!(feedback.message.contains("Toggling USB gamepad"));
    }

    #[test]
    fn test_terminal_detail_marks_output_row_as_selected() {
        let detail = USB_GAMEPAD_APP.render_terminal_detail(&UiFrame::default());

        assert!(detail.contains("> Output: OFF"));
    }
}
