#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiInputEvent {
    Left,
    Right,
    Up,
    Down,
    Open,
    Back,
    PagePrev,
    PageNext,
    Quit,
}

#[rpos::ctor::ctor]
fn register_ui_input_message() {
    rpos::msg::add_message::<UiInputEvent>("ui_input_event");
}
