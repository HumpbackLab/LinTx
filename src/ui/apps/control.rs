use crate::ui::{
    apps::{common::format_channel_groups, AppSpec, UiAppContext, UiAppModule},
    input::UiInputEvent,
    model::{AppId, UiFrame},
};

pub const SPEC: AppSpec = AppSpec {
    id: AppId::Control,
    title: "CONTROL",
    icon_text: "CTL",
    accent: (86, 214, 165),
};

pub struct ControlApp;
pub static CONTROL_APP: ControlApp = ControlApp;

impl UiAppModule for ControlApp {
    fn on_event(&self, _frame: &mut UiFrame, _event: UiInputEvent, _ctx: &UiAppContext<'_>) {}

    fn render_terminal_detail(&self, frame: &UiFrame) -> String {
        format!(
            "Input Source: {}\nStatus: {} ({})\nELRS Feedback: {}  Signal:{}  Aircraft Battery:{}\nELRS Detail: {}\nChannels: {}\n{}\n\nMixer Out (0..10000)\nCH1 Roll:{}\nCH2 Pitch:{}\nCH3 Throttle:{}\nCH4 Yaw:{}\nCH5 Arm:{}\nCH6 FlightMode:{}\nCH7 Beeper:{}\nCH8 Turtle:{}\n\nUse this page to validate input chain.\nEsc Back",
            frame.input_status.source.label(),
            frame.input_status.health.label(),
            frame.input_status.detail,
            if frame.elrs_feedback.connected {
                "connected"
            } else {
                "disconnected"
            },
            frame
                .elrs_feedback
                .signal_strength_percent
                .map(|v| format!("{}%", v))
                .unwrap_or_else(|| "--".to_string()),
            frame
                .elrs_feedback
                .aircraft_battery_percent
                .map(|v| format!("{}%", v))
                .unwrap_or_else(|| "--".to_string()),
            frame.elrs_feedback.detail,
            frame.input_frame.channels.len(),
            format_channel_groups(&frame.input_frame.channels),
            frame.mixer_out.channels[0],
            frame.mixer_out.channels[1],
            frame.mixer_out.channels[2],
            frame.mixer_out.channels[3],
            frame.mixer_out.channels[4],
            frame.mixer_out.channels[5],
            frame.mixer_out.channels[6],
            frame.mixer_out.channels[7],
        )
    }
}
