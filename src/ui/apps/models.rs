use crate::{
    config::store,
    messages::{
        ActiveModelMsg, UiFeedbackMotion, UiFeedbackSeverity, UiFeedbackSlot, UiFeedbackTarget,
        UiInteractionFeedback,
    },
    ui::{
        apps::{AppSpec, UiAppContext, UiAppModule},
        debug_log,
        input::UiInputEvent,
        model::{AppId, UiFrame},
    },
};

pub const SPEC: AppSpec = AppSpec {
    id: AppId::Models,
    title: "MODELS",
    icon_text: "MOD",
    accent: (255, 181, 92),
};

pub struct ModelsApp;
pub static MODELS_APP: ModelsApp = ModelsApp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisibleModelRow {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub is_focused: bool,
    pub is_active: bool,
}

fn build_model_apply_feedback(seq: u32, message: impl Into<String>) -> UiInteractionFeedback {
    UiInteractionFeedback {
        seq,
        severity: UiFeedbackSeverity::Success,
        target: UiFeedbackTarget::SelectedListRow,
        motion: UiFeedbackMotion::Pulse,
        slot: UiFeedbackSlot::TopStatusBar,
        message: message.into(),
        ttl_ms: 850,
    }
}

fn next_model_feedback_seq(frame: &UiFrame) -> u32 {
    frame
        .interaction_feedback
        .as_ref()
        .map(|feedback| feedback.seq.wrapping_add(1))
        .unwrap_or(1)
}

fn visible_model_start(frame: &UiFrame, rows: usize) -> usize {
    if frame.model_entries.is_empty() || rows == 0 {
        return 0;
    }

    let max_idx = frame.model_entries.len().saturating_sub(1);
    let focus = frame.model_focus_idx.min(max_idx);
    focus
        .saturating_sub(1)
        .min(max_idx.saturating_sub(rows.saturating_sub(1)))
}

pub(crate) fn visible_model_rows(frame: &UiFrame, rows: usize) -> Vec<VisibleModelRow> {
    if frame.model_entries.is_empty() || rows == 0 {
        return Vec::new();
    }

    let max_idx = frame.model_entries.len().saturating_sub(1);
    let start = visible_model_start(frame, rows);
    let end = (start + rows).min(frame.model_entries.len());
    let focus = frame.model_focus_idx.min(max_idx);
    let active = frame.model_active_idx.min(max_idx);

    frame.model_entries[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| {
            let idx = start + offset;
            VisibleModelRow {
                id: entry.id.clone(),
                name: entry.name.clone(),
                protocol: entry.protocol.clone(),
                is_focused: idx == focus,
                is_active: idx == active,
            }
        })
        .collect()
}

impl UiAppModule for ModelsApp {
    fn on_event(&self, frame: &mut UiFrame, event: UiInputEvent, ctx: &UiAppContext<'_>) {
        match event {
            UiInputEvent::Up => {
                frame.model_focus_idx = frame.model_focus_idx.saturating_sub(1);
            }
            UiInputEvent::Down => {
                let max_idx = frame.model_entries.len().saturating_sub(1);
                frame.model_focus_idx = (frame.model_focus_idx + 1).min(max_idx);
            }
            UiInputEvent::Open => {
                if let Some(entry) = frame.model_entries.get(frame.model_focus_idx) {
                    match store::set_active_model(&entry.id) {
                        Ok(_) => {
                            frame.model_active_idx = frame.model_focus_idx;
                            ctx.ui_feedback_tx.send(build_model_apply_feedback(
                                next_model_feedback_seq(frame),
                                format!("Profile {} applied", entry.name),
                            ));
                            match store::load_active_model() {
                                Ok(model) => ctx.active_model_tx.send(ActiveModelMsg { model }),
                                Err(err) => debug_log(&format!("load_active_model failed: {err}")),
                            }
                        }
                        Err(err) => {
                            let message = format!("Apply profile failed: {err}");
                            ctx.ui_feedback_tx.send(UiInteractionFeedback {
                                seq: next_model_feedback_seq(frame),
                                severity: UiFeedbackSeverity::Error,
                                target: UiFeedbackTarget::SelectedListRow,
                                motion: UiFeedbackMotion::ShakeX,
                                slot: UiFeedbackSlot::TopStatusBar,
                                message: message.clone(),
                                ttl_ms: 900,
                            });
                            debug_log(&format!("set_active_model failed: {err}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn render_terminal_detail(&self, frame: &UiFrame) -> String {
        if frame.model_entries.is_empty() {
            return "No model entries found.\n\nEsc Back".to_string();
        }

        let max_idx = frame.model_entries.len().saturating_sub(1);
        let focus = frame.model_focus_idx.min(max_idx);
        let active = frame.model_active_idx.min(max_idx);
        let focus_model = &frame.model_entries[focus];
        let active_model = &frame.model_entries[active];

        let mut lines: Vec<String> = visible_model_rows(frame, 4)
            .into_iter()
            .map(|row| {
                format!(
                    "{} {} [{} · {}]",
                    if row.is_focused { ">" } else { " " },
                    row.name,
                    if row.is_active { "ACTIVE" } else { "STORED" },
                    row.protocol,
                )
            })
            .collect();

        while lines.len() < 4 {
            lines.push(String::new());
        }

        format!(
            "ACTIVE: {} ({})\nFOCUSED: {} ({})\n\nProfiles\n{}\n{}\n{}\n{}\n\nUp/Down: focus profile\nEnter: apply focused profile\nEsc Back",
            active_model.name,
            active_model.protocol,
            focus_model.name,
            focus_model.protocol,
            lines[0],
            lines[1],
            lines[2],
            lines[3],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{visible_model_rows, MODELS_APP};
    use crate::{
        config::store,
        messages::{UiFeedbackSeverity, UiFeedbackSlot},
        ui::{
            apps::{UiAppContext, UiAppModule},
            model::{UiFrame, UiModelEntry},
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
            let test_dir = env::temp_dir().join(format!("lintx-model-feedback-{unique}"));
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

    #[test]
    fn test_render_terminal_detail_uses_explicit_active_label() {
        let mut frame = UiFrame::default();
        frame.model_entries = vec![
            UiModelEntry {
                id: "quad_x".to_string(),
                name: "Quad X".to_string(),
                protocol: "CRSF".to_string(),
            },
            UiModelEntry {
                id: "rover".to_string(),
                name: "Rover".to_string(),
                protocol: "USB HID".to_string(),
            },
        ];
        frame.model_active_idx = 0;
        frame.model_focus_idx = 1;

        let detail = MODELS_APP.render_terminal_detail(&frame);

        assert!(detail.contains("ACTIVE: Quad X (CRSF)"));
        assert!(detail.contains("FOCUSED: Rover (USB HID)"));
        assert!(!detail.contains("[A]"));
        assert!(detail.contains("> Rover [STORED · USB HID]"));
    }

    #[test]
    fn test_visible_model_rows_follow_focus_window() {
        let mut frame = UiFrame::default();
        frame.model_entries = vec![
            UiModelEntry {
                id: "m1".to_string(),
                name: "Model 1".to_string(),
                protocol: "P1".to_string(),
            },
            UiModelEntry {
                id: "m2".to_string(),
                name: "Model 2".to_string(),
                protocol: "P2".to_string(),
            },
            UiModelEntry {
                id: "m3".to_string(),
                name: "Model 3".to_string(),
                protocol: "P3".to_string(),
            },
            UiModelEntry {
                id: "m4".to_string(),
                name: "Model 4".to_string(),
                protocol: "P4".to_string(),
            },
            UiModelEntry {
                id: "m5".to_string(),
                name: "Model 5".to_string(),
                protocol: "P5".to_string(),
            },
        ];
        frame.model_active_idx = 1;
        frame.model_focus_idx = 4;

        let rows = visible_model_rows(&frame, 4);

        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].id, "m2");
        assert!(rows[0].is_active);
        assert_eq!(rows[3].id, "m5");
        assert!(rows[3].is_focused);
    }

    #[test]
    fn test_open_applies_model_and_emits_success_feedback() {
        let _serial = store::TEST_CWD_MUTEX.lock().unwrap();
        let _guard = TestCwdGuard::new();
        store::ensure_default_layout().expect("default layout");

        let (config_tx, _config_rx) = Channel::new();
        let (active_model_tx, _active_model_rx) = Channel::new();
        let (elrs_cmd_tx, _elrs_cmd_rx) = Channel::new();
        let (ui_feedback_tx, mut ui_feedback_rx) = Channel::new();
        let (usb_gamepad_cmd_tx, _usb_gamepad_cmd_rx) = Channel::new();
        let ctx = UiAppContext {
            config_tx: &config_tx,
            active_model_tx: &active_model_tx,
            elrs_cmd_tx: &elrs_cmd_tx,
            ui_feedback_tx: &ui_feedback_tx,
            usb_gamepad_cmd_tx: &usb_gamepad_cmd_tx,
        };

        let mut frame = UiFrame::default();
        frame.model_entries = vec![
            UiModelEntry {
                id: "quad_x".to_string(),
                name: "Quad X".to_string(),
                protocol: "CRSF".to_string(),
            },
            UiModelEntry {
                id: "rover".to_string(),
                name: "Rover".to_string(),
                protocol: "USB HID".to_string(),
            },
        ];
        frame.model_focus_idx = 1;
        frame.model_active_idx = 0;

        MODELS_APP.on_event(&mut frame, crate::ui::input::UiInputEvent::Open, &ctx);

        let feedback = ui_feedback_rx.try_read().expect("ui feedback");
        assert_eq!(feedback.severity, UiFeedbackSeverity::Success);
        assert_eq!(feedback.slot, UiFeedbackSlot::TopStatusBar);
        assert!(feedback.message.contains("Rover"));
        assert_eq!(frame.model_active_idx, 1);
    }
}
