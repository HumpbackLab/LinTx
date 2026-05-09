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
            "Input Source: {}\nStatus: {} ({})\nELRS Feedback: {}  Signal:{}  Aircraft Battery:{}\nELRS Detail: {}\nRaw Input\n{}\n\nMixer Out (0..10000)\nMixer CH1-4: {}/{}/{}/{}\nMixer CH5-8: {}/{}/{}/{}\nMixer CH9-12: {}/{}/{}/{}\n\nUse this page to validate input chain.\nEsc Back",
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
            format_channel_groups(&frame.input_frame.channels),
            frame.mixer_out.channels[0],
            frame.mixer_out.channels[1],
            frame.mixer_out.channels[2],
            frame.mixer_out.channels[3],
            frame.mixer_out.channels[4],
            frame.mixer_out.channels[5],
            frame.mixer_out.channels[6],
            frame.mixer_out.channels[7],
            frame.mixer_out.channels[8],
            frame.mixer_out.channels[9],
            frame.mixer_out.channels[10],
            frame.mixer_out.channels[11],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::CONTROL_APP;
    use crate::{
        mixer::MixerOutMsg,
        ui::{apps::UiAppModule, model::UiFrame},
    };

    #[test]
    fn test_terminal_detail_omits_raw_count_and_shows_aux_mixer_groups() {
        let mut frame = UiFrame::default();
        frame.mixer_out = MixerOutMsg {
            channels: [
                100, 200, 300, 400, 5000, 6000, 7000, 8000, 9000, 10000, 1100, 1200, 0, 0, 0, 0,
            ],
        };

        let detail = CONTROL_APP.render_terminal_detail(&frame);

        assert!(!detail.contains("Count:"));
        assert!(detail.contains("Mixer CH5-8: 5000/6000/7000/8000"));
        assert!(detail.contains("Mixer CH9-12: 9000/10000/1100/1200"));
    }
}
