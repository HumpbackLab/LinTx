#[cfg(target_os = "linux")]
use std::{
    ffi::CString,
    fs::OpenOptions,
    io::{BufRead, BufReader},
    os::fd::AsRawFd,
    os::unix::fs::FileTypeExt,
    path::Path,
    time::Duration,
};

use clap::Parser;
use rpos::{msg::get_new_tx_of_message, thread_logln};

use crate::{client_process_args, ui::input::UiInputEvent};

#[derive(Parser)]
#[command(
    name = "ui_input_fifo",
    about = "Read UI key events from a FIFO and inject into UI bus",
    long_about = None
)]
struct Cli {
    #[arg(long, default_value = "/tmp/lintx-ui-input.fifo")]
    pipe_path: String,
}

#[cfg(target_os = "linux")]
fn ensure_fifo(path: &Path) -> Result<(), String> {
    if path.exists() {
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if meta.file_type().is_fifo() {
            return Ok(());
        }
        return Err(format!("{} exists and is not a FIFO", path.display()));
    }

    let c_path = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("invalid fifo path: {}", path.display()))?;
    let ret = unsafe { libc::mkfifo(c_path.as_ptr(), 0o666) };
    if ret != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

fn parse_event_token(token: &str) -> Option<UiInputEvent> {
    let t = token.trim().to_ascii_lowercase();
    match t.as_str() {
        "up" | "w" | "k" => Some(UiInputEvent::Up),
        "down" | "s" | "j" => Some(UiInputEvent::Down),
        "left" | "a" | "h" => Some(UiInputEvent::Left),
        "right" | "d" | "l" => Some(UiInputEvent::Right),
        "open" | "enter" => Some(UiInputEvent::Open),
        "back" | "esc" | "escape" | "b" => Some(UiInputEvent::Back),
        "page-prev" | "pageprev" | "[" => Some(UiInputEvent::PagePrev),
        "page-next" | "pagenext" | "]" => Some(UiInputEvent::PageNext),
        "quit" | "q" => Some(UiInputEvent::Quit),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
pub fn ui_input_fifo_main(argc: u32, argv: *const &str) {
    let args = match client_process_args::<Cli>(argc, argv) {
        Some(a) => a,
        None => return,
    };

    let path = Path::new(&args.pipe_path);
    let lock_path = format!("{}.lock", &args.pipe_path);
    let lock_file = match OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(err) => {
            thread_logln!("ui_input_fifo lock open failed: {}", err);
            return;
        }
    };
    let lock_ret = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if lock_ret != 0 {
        thread_logln!("ui_input_fifo already running for {}", args.pipe_path);
        return;
    }

    if let Err(err) = ensure_fifo(path) {
        thread_logln!("ui_input_fifo setup failed: {}", err);
        return;
    }

    let tx = get_new_tx_of_message::<UiInputEvent>("ui_input_event").unwrap();
    thread_logln!("ui_input_fifo listening on {}", args.pipe_path);

    loop {
        let fifo = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(err) => {
                thread_logln!("ui_input_fifo open failed: {}", err);
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
        };
        let mut reader = BufReader::new(fifo);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(_) => {
                    for token in line.split_whitespace() {
                        if let Some(evt) = parse_event_token(token) {
                            tx.send(evt);
                        }
                    }
                }
                Err(err) => {
                    thread_logln!("ui_input_fifo read failed: {}", err);
                    break;
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn ui_input_fifo_main(argc: u32, argv: *const &str) {
    let _ = client_process_args::<Cli>(argc, argv);
    rpos::thread_logln!("ui_input_fifo is only supported on Linux");
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("ui_input_fifo", ui_input_fifo_main);
}
