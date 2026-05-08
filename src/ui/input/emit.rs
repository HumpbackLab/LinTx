use clap::{Parser, ValueEnum};
use rpos::msg::get_new_tx_of_message;

use crate::{client_process_args, ui::input::UiInputEvent};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EventArg {
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

impl From<EventArg> for UiInputEvent {
    fn from(value: EventArg) -> Self {
        match value {
            EventArg::Left => UiInputEvent::Left,
            EventArg::Right => UiInputEvent::Right,
            EventArg::Up => UiInputEvent::Up,
            EventArg::Down => UiInputEvent::Down,
            EventArg::Open => UiInputEvent::Open,
            EventArg::Back => UiInputEvent::Back,
            EventArg::PagePrev => UiInputEvent::PagePrev,
            EventArg::PageNext => UiInputEvent::PageNext,
            EventArg::Quit => UiInputEvent::Quit,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "ui_emit_input",
    about = "Emit one UI input event into LinTx message bus",
    long_about = None
)]
struct Cli {
    #[arg(long, value_enum)]
    event: EventArg,
}

pub fn ui_emit_input_main(argc: u32, argv: *const &str) {
    let args = match client_process_args::<Cli>(argc, argv) {
        Some(a) => a,
        None => return,
    };

    let tx = get_new_tx_of_message::<UiInputEvent>("ui_input_event").unwrap();
    tx.send(args.event.into());
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("ui_emit_input", ui_emit_input_main);
}
