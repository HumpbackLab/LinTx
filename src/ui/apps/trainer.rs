use crate::{
    config::{store, AuxSource, ModelConfig},
    messages::{
        ActiveModelMsg, UiFeedbackMotion, UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget,
        UiInteractionFeedback,
    },
    ui::{
        apps::{AppSpec, UiAppContext, UiAppModule},
        input::UiInputEvent,
        keyboard::{KeyboardField, KeyboardOverlay},
        model::{AppId, UiFrame},
    },
};

pub const SPEC: AppSpec = AppSpec {
    id: AppId::Trainer,
    title: "AUX MAP",
    icon_text: "AUX",
    accent: (255, 123, 118),
};

pub struct TrainerApp;
pub static TRAINER_APP: TrainerApp = TrainerApp;

const AUX_FIRST_CHANNEL: u8 = 5;
const AUX_LAST_CHANNEL: u8 = 16;
const AUX_VISIBLE_ROWS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisibleAuxRow {
    pub channel: u8,
    pub source: String,
    pub value: u16,
    pub possible_outputs: &'static str,
    pub is_focused: bool,
}

fn load_active_model_or_default() -> ModelConfig {
    store::load_active_model().unwrap_or_default()
}

fn focused_channel(frame: &UiFrame) -> u8 {
    AUX_FIRST_CHANNEL + frame.aux_map_focus_idx.min(aux_count().saturating_sub(1)) as u8
}

fn aux_count() -> usize {
    (AUX_LAST_CHANNEL - AUX_FIRST_CHANNEL + 1) as usize
}

fn visible_start(frame: &UiFrame) -> usize {
    let focus = frame.aux_map_focus_idx.min(aux_count().saturating_sub(1));
    focus
        .saturating_sub(1)
        .min(aux_count().saturating_sub(AUX_VISIBLE_ROWS))
}

pub(crate) fn visible_aux_rows(frame: &UiFrame) -> Vec<VisibleAuxRow> {
    let model = load_active_model_or_default();
    let mappings = model.aux_mapping.normalized_channels();
    let start = visible_start(frame);
    let focus = frame.aux_map_focus_idx.min(aux_count().saturating_sub(1));

    (start..(start + AUX_VISIBLE_ROWS).min(aux_count()))
        .map(|index| {
            let mapping = &mappings[index];
            let value = frame.mixer_out.channels[mapping.channel as usize - 1];
            VisibleAuxRow {
                channel: mapping.channel,
                source: mapping.source.name(),
                value,
                possible_outputs: aux_source_possible_outputs(mapping.source),
                is_focused: index == focus,
            }
        })
        .collect()
}

pub(crate) fn focused_aux_possible_outputs(frame: &UiFrame) -> &'static str {
    visible_aux_rows(frame)
        .into_iter()
        .find(|row| row.is_focused)
        .map(|row| row.possible_outputs)
        .unwrap_or("0")
}

fn aux_source_possible_outputs(source: AuxSource) -> &'static str {
    match source {
        AuxSource::None => "0",
        AuxSource::Switch3Pos(_) => "0/5000/10000",
        AuxSource::Switch2Pos(_) | AuxSource::Button(_) => "0/10000",
    }
}

fn feedback(
    seq: u32,
    severity: UiFeedbackSeverity,
    message: impl Into<String>,
) -> UiInteractionFeedback {
    UiInteractionFeedback {
        seq,
        severity,
        target: UiFeedbackTarget::SelectedListRow,
        motion: match severity {
            UiFeedbackSeverity::Error => UiFeedbackMotion::ShakeX,
            _ => UiFeedbackMotion::Pulse,
        },
        slot: UiFeedbackSlot::TopStatusBar,
        message: message.into(),
        ttl_ms: 900,
    }
}

fn next_feedback_seq(frame: &UiFrame) -> u32 {
    frame
        .interaction_feedback
        .as_ref()
        .map(|feedback| feedback.seq.wrapping_add(1))
        .unwrap_or(1)
}

impl UiAppModule for TrainerApp {
    fn on_event(&self, frame: &mut UiFrame, event: UiInputEvent, ctx: &UiAppContext<'_>) {
        match event {
            UiInputEvent::Up => {
                frame.aux_map_focus_idx = frame.aux_map_focus_idx.saturating_sub(1);
                frame.keyboard_armed_field = None;
            }
            UiInputEvent::Down => {
                frame.aux_map_focus_idx =
                    (frame.aux_map_focus_idx + 1).min(aux_count().saturating_sub(1));
                frame.keyboard_armed_field = None;
            }
            UiInputEvent::Open => {
                let channel = focused_channel(frame);
                let field = KeyboardField::AuxSource { channel };
                if frame.keyboard_armed_field != Some(field) {
                    frame.keyboard_armed_field = Some(field);
                    ctx.ui_feedback_tx.send(feedback(
                        next_feedback_seq(frame),
                        UiFeedbackSeverity::Busy,
                        "Click to edit",
                    ));
                    return;
                }
                let source = load_active_model_or_default()
                    .aux_mapping
                    .normalized_channels()
                    .get(channel as usize - AUX_FIRST_CHANNEL as usize)
                    .map(|mapping| mapping.source.name())
                    .unwrap_or_else(|| "none".to_string());
                frame.keyboard = Some(KeyboardOverlay::aux_source(channel, source));
                frame.keyboard_armed_field = None;
            }
            UiInputEvent::Back
            | UiInputEvent::Left
            | UiInputEvent::Right
            | UiInputEvent::PagePrev
            | UiInputEvent::PageNext
            | UiInputEvent::KeyboardTap { .. }
            | UiInputEvent::KeyboardSubmit
            | UiInputEvent::Quit => {
                frame.keyboard_armed_field = None;
            }
        }
    }

    fn render_terminal_detail(&self, frame: &UiFrame) -> String {
        let mut lines: Vec<String> = visible_aux_rows(frame)
            .into_iter()
            .map(|row| {
                format!(
                    "{} CH{}: [{}] {}",
                    if row.is_focused { ">" } else { " " },
                    row.channel,
                    row.source.to_ascii_uppercase(),
                    row.value,
                )
            })
            .collect();
        while lines.len() < AUX_VISIBLE_ROWS {
            lines.push(String::new());
        }
        format!(
            "AUX Channel Map\n{}\n{}\n{}\n{}\n\nUp/Down: focus CH5-CH16\nEnter: edit source\nEsc Back",
            lines[0], lines[1], lines[2], lines[3],
        )
    }

    fn on_keyboard_submit(
        &self,
        frame: &mut UiFrame,
        field: KeyboardField,
        value: &str,
        ctx: &UiAppContext<'_>,
    ) -> bool {
        let KeyboardField::AuxSource { channel } = field else {
            return false;
        };
        if !(AUX_FIRST_CHANNEL..=AUX_LAST_CHANNEL).contains(&channel) {
            return false;
        }

        let Some(source) = AuxSource::parse_name(value) else {
            ctx.ui_feedback_tx.send(feedback(
                next_feedback_seq(frame),
                UiFeedbackSeverity::Error,
                format!("Invalid source {value}"),
            ));
            return false;
        };

        let mut model = load_active_model_or_default();
        let mut channels = model.aux_mapping.normalized_channels();
        let index = channel as usize - AUX_FIRST_CHANNEL as usize;
        channels[index].source = source;
        channels[index].inverted = false;
        model.aux_mapping.channels = channels;

        match store::save_model_config(&model) {
            Ok(()) => {
                ctx.active_model_tx.send(ActiveModelMsg {
                    model: model.clone(),
                });
                ctx.ui_feedback_tx.send(feedback(
                    next_feedback_seq(frame),
                    UiFeedbackSeverity::Success,
                    format!("CH{channel} source saved"),
                ));
                true
            }
            Err(err) => {
                ctx.ui_feedback_tx.send(feedback(
                    next_feedback_seq(frame),
                    UiFeedbackSeverity::Error,
                    format!("AUX map save failed: {err}"),
                ));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SPEC, TRAINER_APP};
    use crate::{
        config::{store, AuxSource},
        messages::{ActiveModelMsg, ElrsCommandMsg, SystemConfigMsg, UsbGamepadCommandMsg},
        ui::{
            apps::{UiAppContext, UiAppModule},
            input::UiInputEvent,
            keyboard::KeyboardField,
            model::UiFrame,
        },
    };
    use rpos::channel::Channel;
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestCwdGuard {
        original: PathBuf,
        test_dir: PathBuf,
    }

    impl TestCwdGuard {
        fn new() -> Self {
            let original = env::current_dir().expect("cwd");
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos();
            let test_dir = env::temp_dir().join(format!("lintx-aux-map-{unique}"));
            fs::create_dir_all(&test_dir).expect("create test dir");
            env::set_current_dir(&test_dir).expect("chdir test dir");
            Self { original, test_dir }
        }
    }

    impl Drop for TestCwdGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
            let _ = fs::remove_dir_all(&self.test_dir);
        }
    }

    fn context<'a>(
        config_tx: &'a rpos::channel::Sender<SystemConfigMsg>,
        active_model_tx: &'a rpos::channel::Sender<ActiveModelMsg>,
        elrs_cmd_tx: &'a rpos::channel::Sender<ElrsCommandMsg>,
        ui_feedback_tx: &'a rpos::channel::Sender<crate::messages::UiInteractionFeedback>,
        usb_gamepad_cmd_tx: &'a rpos::channel::Sender<UsbGamepadCommandMsg>,
    ) -> UiAppContext<'a> {
        UiAppContext {
            config_tx,
            active_model_tx,
            elrs_cmd_tx,
            ui_feedback_tx,
            usb_gamepad_cmd_tx,
        }
    }

    #[test]
    fn test_spec_replaces_trainer_with_aux_map() {
        assert_eq!(SPEC.title, "AUX MAP");
        assert_eq!(SPEC.icon_text, "AUX");
    }

    #[test]
    fn test_open_starts_editing_focused_aux_channel() {
        let _serial = store::TEST_CWD_MUTEX.lock().unwrap();
        let mut frame = UiFrame::default();
        frame.aux_map_focus_idx = 1;
        let (config_tx, _config_rx) = Channel::new();
        let (active_model_tx, _active_model_rx) = Channel::new();
        let (elrs_cmd_tx, _elrs_cmd_rx) = Channel::new();
        let (ui_feedback_tx, mut ui_feedback_rx) = Channel::new();
        let (usb_gamepad_cmd_tx, _usb_gamepad_cmd_rx) = Channel::new();
        let ctx = context(
            &config_tx,
            &active_model_tx,
            &elrs_cmd_tx,
            &ui_feedback_tx,
            &usb_gamepad_cmd_tx,
        );

        TRAINER_APP.on_event(&mut frame, UiInputEvent::Open, &ctx);

        assert!(frame.keyboard.is_none());
        assert_eq!(
            frame.keyboard_armed_field,
            Some(KeyboardField::AuxSource { channel: 6 })
        );
        assert_eq!(
            ui_feedback_rx.try_read().expect("feedback").message,
            "Click to edit"
        );

        TRAINER_APP.on_event(&mut frame, UiInputEvent::Open, &ctx);

        let keyboard = frame.keyboard.expect("keyboard");
        assert_eq!(keyboard.field, KeyboardField::AuxSource { channel: 6 });
        assert_eq!(keyboard.buffer, "SA");
        assert_eq!(frame.keyboard_armed_field, None);
    }

    #[test]
    fn test_aux_map_terminal_rows_keep_channel_source_and_value_separate() {
        let _serial = store::TEST_CWD_MUTEX.lock().unwrap();
        let mut frame = UiFrame::default();
        frame.mixer_out.channels[4] = 10000;

        let detail = TRAINER_APP.render_terminal_detail(&frame);

        assert!(detail.contains("> CH5: [S1] 10000"));
        assert!(!detail.contains("CH5 S1 = 10000"));
    }

    #[test]
    fn test_submit_valid_aux_source_saves_active_model_and_publishes_update() {
        let _serial = store::TEST_CWD_MUTEX.lock().unwrap();
        let _guard = TestCwdGuard::new();
        store::ensure_default_layout().unwrap();
        let mut frame = UiFrame::default();
        let (config_tx, _config_rx) = Channel::new();
        let (active_model_tx, mut active_model_rx) = Channel::new();
        let (elrs_cmd_tx, _elrs_cmd_rx) = Channel::new();
        let (ui_feedback_tx, _ui_feedback_rx) = Channel::new();
        let (usb_gamepad_cmd_tx, _usb_gamepad_cmd_rx) = Channel::new();
        let ctx = context(
            &config_tx,
            &active_model_tx,
            &elrs_cmd_tx,
            &ui_feedback_tx,
            &usb_gamepad_cmd_tx,
        );

        assert!(TRAINER_APP.on_keyboard_submit(
            &mut frame,
            KeyboardField::AuxSource { channel: 16 },
            "B3",
            &ctx,
        ));

        let saved = store::load_active_model().unwrap();
        let channels = saved.aux_mapping.normalized_channels();
        assert_eq!(channels[11].source, AuxSource::Button(3));
        let published = active_model_rx.try_read().expect("active model update");
        assert_eq!(
            published.model.aux_mapping.normalized_channels()[11].source,
            AuxSource::Button(3)
        );
    }

    #[test]
    fn test_submit_invalid_aux_source_is_rejected() {
        let mut frame = UiFrame::default();
        let (config_tx, _config_rx) = Channel::new();
        let (active_model_tx, _active_model_rx) = Channel::new();
        let (elrs_cmd_tx, _elrs_cmd_rx) = Channel::new();
        let (ui_feedback_tx, _ui_feedback_rx) = Channel::new();
        let (usb_gamepad_cmd_tx, _usb_gamepad_cmd_rx) = Channel::new();
        let ctx = context(
            &config_tx,
            &active_model_tx,
            &elrs_cmd_tx,
            &ui_feedback_tx,
            &usb_gamepad_cmd_tx,
        );

        assert!(!TRAINER_APP.on_keyboard_submit(
            &mut frame,
            KeyboardField::AuxSource { channel: 7 },
            "SE",
            &ctx,
        ));
    }
}
