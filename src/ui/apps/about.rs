use crate::ui::{
    apps::{AppSpec, UiAppContext, UiAppModule},
    input::UiInputEvent,
    model::{AppId, UiFrame},
};

pub const SPEC: AppSpec = AppSpec {
    id: AppId::About,
    title: "ABOUT",
    icon_text: "ABT",
    accent: (160, 196, 255),
};

pub struct AboutApp;
pub static ABOUT_APP: AboutApp = AboutApp;

impl UiAppModule for AboutApp {
    fn on_event(&self, _frame: &mut UiFrame, _event: UiInputEvent, _ctx: &UiAppContext<'_>) {}

    fn render_terminal_detail(&self, _frame: &UiFrame) -> String {
        format!(
            "LinTx\n\nTeam: HumpbackLab\nProduct: LinTx\nVersion: 0.0.1 preview\n\nEsc Back",
        )
    }
}
