use rpos::channel::Sender;

use crate::{
    messages::{
        ActiveModelMsg, ElrsCommandMsg, SystemConfigMsg, UiInteractionFeedback,
        UsbGamepadCommandMsg,
    },
    ui::{input::UiInputEvent, keyboard::KeyboardField, model::UiFrame},
};

use super::model::AppId;

mod about;
mod cloud;
mod common;
mod control;
pub(crate) mod models;
mod scripts;
mod system;
mod trainer;
mod usb_gamepad;

#[derive(Debug, Clone, Copy)]
pub struct AppSpec {
    pub id: AppId,
    pub title: &'static str,
    pub icon_text: &'static str,
    pub accent: (u8, u8, u8),
}

#[derive(Debug)]
pub struct PageSpec {
    pub id: usize,
    pub rows: usize,
    pub cols: usize,
    pub apps: &'static [AppId],
}

pub struct UiAppContext<'a> {
    pub config_tx: &'a Sender<SystemConfigMsg>,
    pub active_model_tx: &'a Sender<ActiveModelMsg>,
    pub elrs_cmd_tx: &'a Sender<ElrsCommandMsg>,
    pub ui_feedback_tx: &'a Sender<UiInteractionFeedback>,
    pub usb_gamepad_cmd_tx: &'a Sender<UsbGamepadCommandMsg>,
}

pub trait UiAppModule: Sync {
    fn on_event(&self, frame: &mut UiFrame, event: UiInputEvent, ctx: &UiAppContext<'_>);

    fn render_terminal_detail(&self, frame: &UiFrame) -> String;

    fn intercept_back(&self, _frame: &UiFrame) -> bool {
        false
    }

    fn on_keyboard_submit(
        &self,
        _frame: &mut UiFrame,
        _field: KeyboardField,
        _value: &str,
        _ctx: &UiAppContext<'_>,
    ) -> bool {
        false
    }
}

pub const APP_SPECS: [AppSpec; 8] = [
    system::SPEC,
    control::SPEC,
    models::SPEC,
    cloud::SPEC,
    usb_gamepad::SPEC,
    trainer::SPEC,
    scripts::SPEC,
    about::SPEC,
];

const PAGE0_APPS: [AppId; 4] = [AppId::System, AppId::Control, AppId::Models, AppId::Scripts];
const PAGE1_APPS: [AppId; 8] = [
    AppId::System,
    AppId::Control,
    AppId::Models,
    AppId::Cloud,
    AppId::UsbGamepad,
    AppId::Trainer,
    AppId::Scripts,
    AppId::About,
];

pub static PAGE_SPECS: [PageSpec; 2] = [
    PageSpec {
        id: 0,
        rows: 1,
        cols: 4,
        apps: &PAGE0_APPS,
    },
    PageSpec {
        id: 1,
        rows: 2,
        cols: 4,
        apps: &PAGE1_APPS,
    },
];

pub fn module_of(id: AppId) -> &'static dyn UiAppModule {
    match id {
        AppId::System => &system::SYSTEM_APP,
        AppId::Control => &control::CONTROL_APP,
        AppId::Models => &models::MODELS_APP,
        AppId::Cloud => &cloud::CLOUD_APP,
        AppId::UsbGamepad => &usb_gamepad::USB_GAMEPAD_APP,
        AppId::Trainer => &trainer::TRAINER_APP,
        AppId::Scripts => &scripts::SCRIPTS_APP,
        AppId::About => &about::ABOUT_APP,
    }
}

pub fn app_spec(id: AppId) -> &'static AppSpec {
    APP_SPECS
        .iter()
        .find(|spec| spec.id == id)
        .expect("app spec must exist")
}

pub fn page(page_idx: usize) -> &'static PageSpec {
    &PAGE_SPECS[page_idx % PAGE_SPECS.len()]
}

pub fn app_at(page_idx: usize, row: usize, col: usize) -> Option<AppId> {
    let p = page(page_idx);
    if row >= p.rows || col >= p.cols {
        return None;
    }
    p.apps.get(row * p.cols + col).copied()
}

pub fn handle_event(app: AppId, frame: &mut UiFrame, event: UiInputEvent, ctx: &UiAppContext<'_>) {
    module_of(app).on_event(frame, event, ctx);
}

pub fn handle_keyboard_submit(
    app: AppId,
    frame: &mut UiFrame,
    field: KeyboardField,
    value: &str,
    ctx: &UiAppContext<'_>,
) -> bool {
    module_of(app).on_keyboard_submit(frame, field, value, ctx)
}

pub fn should_intercept_back(app: AppId, frame: &UiFrame) -> bool {
    module_of(app).intercept_back(frame)
}

pub fn render_terminal_detail(app: AppId, frame: &UiFrame) -> String {
    module_of(app).render_terminal_detail(frame)
}
