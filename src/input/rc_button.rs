use std::time::{Duration, Instant};

use rpos::{msg::get_new_tx_of_message, thread_logln};

use crate::{messages::RcInputRawMsg, ui::input::UiInputEvent};

const GUI_UP: u16 = 1 << 10;
const GUI_DOWN: u16 = 1 << 11;
const GUI_LEFT: u16 = 1 << 12;
const GUI_RIGHT: u16 = 1 << 13;
const GUI_CENTER: u16 = 1 << 14;
const GUI_DIRECTION_MASK: u16 = GUI_UP | GUI_DOWN | GUI_LEFT | GUI_RIGHT;
const GUI_MASK: u16 = GUI_DIRECTION_MASK | GUI_CENTER;

const LONG_PRESS_DURATION: Duration = Duration::from_millis(600);
const FIRST_REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy)]
struct HeldDirection {
    bit: u16,
    event: UiInputEvent,
    pressed_at: Instant,
    last_repeat_at: Option<Instant>,
}

#[derive(Debug, Default)]
struct RcButtonMapper {
    previous_buttons: u16,
    held_direction: Option<HeldDirection>,
    center_pressed_at: Option<Instant>,
    center_consumed: bool,
}

impl RcButtonMapper {
    fn new() -> Self {
        Self::default()
    }

    fn update(&mut self, buttons: u32, now: Instant) -> Vec<UiInputEvent> {
        let buttons = (buttons as u16) & GUI_MASK;
        let previous = self.previous_buttons;
        self.previous_buttons = buttons;

        let mut events = Vec::new();
        let center_down = buttons & GUI_CENTER != 0;
        let center_was_down = previous & GUI_CENTER != 0;
        let left_down = buttons & GUI_LEFT != 0;

        if center_down && left_down && !self.center_consumed {
            self.center_consumed = true;
            events.push(UiInputEvent::Back);
        }

        if center_down && !center_was_down {
            self.center_pressed_at = Some(now);
        }

        if center_down {
            if let Some(pressed_at) = self.center_pressed_at {
                if !self.center_consumed
                    && now.saturating_duration_since(pressed_at) >= LONG_PRESS_DURATION
                {
                    self.center_consumed = true;
                    events.push(UiInputEvent::Back);
                }
            }
        } else if center_was_down {
            if !self.center_consumed {
                events.push(UiInputEvent::Open);
            }
            self.center_pressed_at = None;
            self.center_consumed = false;
        }

        if center_down {
            self.held_direction = None;
            return events;
        }

        if let Some((bit, event)) = active_direction(buttons) {
            match self.held_direction {
                Some(mut held) if held.bit == bit => {
                    let repeat_due = held
                        .last_repeat_at
                        .map(|last| now.saturating_duration_since(last) >= REPEAT_INTERVAL)
                        .unwrap_or_else(|| {
                            now.saturating_duration_since(held.pressed_at) >= FIRST_REPEAT_DELAY
                        });
                    if repeat_due {
                        held.last_repeat_at = Some(now);
                        events.push(held.event);
                    }
                    self.held_direction = Some(held);
                }
                _ => {
                    self.held_direction = Some(HeldDirection {
                        bit,
                        event,
                        pressed_at: now,
                        last_repeat_at: None,
                    });
                    events.push(event);
                }
            }
        } else {
            self.held_direction = None;
        }

        events
    }
}

fn active_direction(buttons: u16) -> Option<(u16, UiInputEvent)> {
    if buttons & GUI_UP != 0 {
        Some((GUI_UP, UiInputEvent::Up))
    } else if buttons & GUI_DOWN != 0 {
        Some((GUI_DOWN, UiInputEvent::Down))
    } else if buttons & GUI_LEFT != 0 {
        Some((GUI_LEFT, UiInputEvent::Left))
    } else if buttons & GUI_RIGHT != 0 {
        Some((GUI_RIGHT, UiInputEvent::Right))
    } else {
        None
    }
}

fn rc_button_input_main(_argc: u32, _argv: *const &str) {
    let ui_input_tx = get_new_tx_of_message::<UiInputEvent>("ui_input_event").unwrap();
    let rc_input_rx = match rpos::msg::get_new_rx_of_message::<RcInputRawMsg>("rc_input_raw") {
        Some(rx) => rx,
        None => {
            thread_logln!("rc_button_input failed to subscribe rc_input_raw");
            return;
        }
    };
    let mut mapper = RcButtonMapper::new();
    rc_input_rx.register_callback("rc_button_input", move |msg| {
        for event in mapper.update(msg.buttons, Instant::now()) {
            ui_input_tx.send(event);
        }
    });
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("rc_button_input", rc_button_input_main);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::input::UiInputEvent;
    use std::time::{Duration, Instant};

    fn input(buttons: u16) -> u32 {
        buttons as u32
    }

    #[test]
    fn test_gui_five_way_direction_edges_emit_ui_events() {
        let now = Instant::now();
        let mut mapper = RcButtonMapper::new();

        assert_eq!(mapper.update(input(GUI_UP), now), vec![UiInputEvent::Up]);
        assert!(mapper
            .update(input(GUI_UP), now + Duration::from_millis(10))
            .is_empty());
        assert_eq!(
            mapper.update(0, now + Duration::from_millis(20)),
            Vec::new()
        );
        assert_eq!(
            mapper.update(input(GUI_DOWN), now + Duration::from_millis(30)),
            vec![UiInputEvent::Down]
        );
    }

    #[test]
    fn test_gui_five_way_short_center_press_emits_open_on_release() {
        let now = Instant::now();
        let mut mapper = RcButtonMapper::new();

        assert!(mapper.update(input(GUI_CENTER), now).is_empty());
        assert_eq!(
            mapper.update(0, now + Duration::from_millis(120)),
            vec![UiInputEvent::Open]
        );
    }

    #[test]
    fn test_gui_five_way_long_center_press_emits_back_once() {
        let now = Instant::now();
        let mut mapper = RcButtonMapper::new();

        assert!(mapper.update(input(GUI_CENTER), now).is_empty());
        assert_eq!(
            mapper.update(input(GUI_CENTER), now + LONG_PRESS_DURATION),
            vec![UiInputEvent::Back]
        );
        assert!(mapper
            .update(
                input(GUI_CENTER),
                now + LONG_PRESS_DURATION + Duration::from_millis(20)
            )
            .is_empty());
        assert!(mapper
            .update(0, now + LONG_PRESS_DURATION + Duration::from_millis(40))
            .is_empty());
    }

    #[test]
    fn test_gui_five_way_left_center_combo_emits_back_without_open() {
        let now = Instant::now();
        let mut mapper = RcButtonMapper::new();

        assert_eq!(
            mapper.update(input(GUI_LEFT | GUI_CENTER), now),
            vec![UiInputEvent::Back]
        );
        assert!(mapper
            .update(0, now + Duration::from_millis(100))
            .is_empty());
    }

    #[test]
    fn test_gui_five_way_direction_repeats_after_delay() {
        let now = Instant::now();
        let mut mapper = RcButtonMapper::new();

        assert_eq!(
            mapper.update(input(GUI_RIGHT), now),
            vec![UiInputEvent::Right]
        );
        assert!(mapper
            .update(
                input(GUI_RIGHT),
                now + FIRST_REPEAT_DELAY - Duration::from_millis(1)
            )
            .is_empty());
        assert_eq!(
            mapper.update(input(GUI_RIGHT), now + FIRST_REPEAT_DELAY),
            vec![UiInputEvent::Right]
        );
        assert!(mapper
            .update(
                input(GUI_RIGHT),
                now + FIRST_REPEAT_DELAY + REPEAT_INTERVAL - Duration::from_millis(1)
            )
            .is_empty());
        assert_eq!(
            mapper.update(input(GUI_RIGHT), now + FIRST_REPEAT_DELAY + REPEAT_INTERVAL),
            vec![UiInputEvent::Right]
        );
    }
}
