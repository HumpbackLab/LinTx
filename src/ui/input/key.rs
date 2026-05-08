use clap::Parser;
use rpos::{msg::get_new_tx_of_message, thread_logln};

use crate::{client_process_args, ui::input::UiInputEvent};

#[derive(Parser)]
#[command(
    name = "ui_key_input",
    about = "Read keyboard input from terminal and inject UI events",
    long_about = None
)]
struct Cli {}

#[cfg(target_os = "linux")]
fn read_byte_with_timeout(fd: libc::c_int, timeout_ms: i32) -> std::io::Result<Option<u8>> {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, timeout_ms) };
    if ready < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if ready == 0 {
        return Ok(None);
    }

    let mut ch = [0u8; 1];
    let read_n = unsafe { libc::read(fd, ch.as_mut_ptr() as *mut libc::c_void, 1) };
    if read_n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if read_n == 0 {
        return Ok(None);
    }
    Ok(Some(ch[0]))
}

#[cfg(target_os = "linux")]
fn decode_event(fd: libc::c_int, first: u8) -> std::io::Result<Option<UiInputEvent>> {
    let event = match first {
        b'q' | b'Q' => Some(UiInputEvent::Quit),
        b'\r' | b'\n' => Some(UiInputEvent::Open),
        b'[' => Some(UiInputEvent::PagePrev),
        b']' => Some(UiInputEvent::PageNext),
        b'w' | b'W' | b'k' | b'K' => Some(UiInputEvent::Up),
        b's' | b'S' | b'j' | b'J' => Some(UiInputEvent::Down),
        b'a' | b'A' | b'h' | b'H' => Some(UiInputEvent::Left),
        b'd' | b'D' | b'l' | b'L' => Some(UiInputEvent::Right),
        b'b' | b'B' => Some(UiInputEvent::Back),
        0x1b => {
            let second = read_byte_with_timeout(fd, 25)?;
            match second {
                None => Some(UiInputEvent::Back),
                Some(b'[') | Some(b'O') => {
                    let third = read_byte_with_timeout(fd, 25)?;
                    match third {
                        Some(b'A') => Some(UiInputEvent::Up),
                        Some(b'B') => Some(UiInputEvent::Down),
                        Some(b'C') => Some(UiInputEvent::Right),
                        Some(b'D') => Some(UiInputEvent::Left),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    };

    Ok(event)
}

#[cfg(target_os = "linux")]
pub fn ui_key_input_main(argc: u32, argv: *const &str) {
    if client_process_args::<Cli>(argc, argv).is_none() {
        return;
    }

    if rpos::server_client::setup_client_stdin_out().is_err() {
        thread_logln!("ui_key_input failed to attach client stdin/stdout");
        return;
    }

    let tx = get_new_tx_of_message::<UiInputEvent>("ui_input_event").unwrap();
    let fd = libc::STDIN_FILENO;
    if unsafe { libc::isatty(fd) } == 0 {
        thread_logln!("ui_key_input requires a TTY stdin");
        return;
    }

    thread_logln!("ui_key_input started");
    thread_logln!("keys: arrows/WASD move, Enter=open, Esc/b=back, [ ]=page, q=quit");

    loop {
        let first = match read_byte_with_timeout(fd, -1) {
            Ok(Some(ch)) => ch,
            Ok(None) => continue,
            Err(err) => {
                thread_logln!("ui_key_input read error: {}", err);
                break;
            }
        };

        let event = match decode_event(fd, first) {
            Ok(evt) => evt,
            Err(err) => {
                thread_logln!("ui_key_input decode error: {}", err);
                break;
            }
        };

        if let Some(evt) = event {
            tx.send(evt);
            if evt == UiInputEvent::Quit {
                break;
            }
        }
    }

    thread_logln!("ui_key_input stopped");
}

#[cfg(not(target_os = "linux"))]
pub fn ui_key_input_main(argc: u32, argv: *const &str) {
    let _ = client_process_args::<Cli>(argc, argv);
    rpos::thread_logln!("ui_key_input is only supported on Linux");
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("ui_key_input", ui_key_input_main);
}
