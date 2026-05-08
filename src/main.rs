use clap::Parser;
#[cfg(target_os = "windows")]
use rpos::module::Module;
#[cfg(not(target_os = "windows"))]
use rpos::server_client::{server_init, Client};
#[cfg(not(target_os = "windows"))]
use std::io::ErrorKind;
mod config;
mod control;
mod input;
mod messages;
#[cfg(target_os = "linux")]
mod output;
mod tools;
mod ui;

pub use control::mixer;
#[cfg(target_os = "linux")]
pub use output::elrs;

pub const CALIBRATE_FILENAME: &str = "joystick.toml";

#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help(true))]
struct Cli {
    #[arg(long)]
    server: bool,

    #[arg(long, help = "Run client command detached from this terminal/client")]
    detach: bool,

    /// commands send by clients.
    #[arg(value_name = "client commands")]
    other: Option<Vec<String>>,
}

pub fn client_process_args<T: clap::Parser>(argc: u32, argv: *const &str) -> Option<T> {
    let argv = unsafe { std::slice::from_raw_parts(argv, argc as usize) };

    let ret = T::try_parse_from(argv);

    if ret.is_err() {
        let help_str = T::command().render_help();
        rpos::thread_logln!("{}", help_str);
        return None;
    }
    ret.ok()
}

fn main() {
    if let Err(err) = crate::config::store::ensure_default_layout() {
        eprintln!("[lintx] failed to initialize config layout: {}", err);
    }
    #[cfg(target_os = "windows")]
    {
        run_windows_local_mode();
        return;
    }

    #[cfg(not(target_os = "windows"))]
    run_unix_client_server();
}

#[cfg(not(target_os = "windows"))]
fn debug_enabled() -> bool {
    std::env::var("LINTX_DEBUG")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn debug_log(msg: &str) {
    if debug_enabled() {
        eprintln!("[lintx-debug] {msg}");
    }
}

#[cfg(not(target_os = "windows"))]
fn socket_path() -> String {
    let from_env = std::env::var("LINTX_SOCKET_PATH")
        .ok()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty());

    from_env.unwrap_or_else(|| "/tmp/lintx-rpsocket".to_string())
}

#[cfg(not(target_os = "windows"))]
fn run_unix_client_server() {
    let socket_path = socket_path();
    let cli = Cli::parse();
    debug_log(&format!(
        "parsed cli: server={} detach={} socket_path={}",
        cli.server, cli.detach, socket_path
    ));

    if cli.server {
        let hello_txt = r"
        __    _     ______    
       / /   (_)___/_  __/  __
      / /   / / __ \/ / | |/_/
     / /___/ / / / / / _>  <  
    /_____/_/_/ /_/_/ /_/|_|  ";

        println!("{hello_txt}");

        println!(
            "Built from branch={} commit={} dirty={} source_timestamp={}",
            env!("GIT_BRANCH"),
            env!("GIT_COMMIT"),
            env!("GIT_DIRTY"),
            env!("SOURCE_TIMESTAMP"),
        );
        println!("Server socket path: {socket_path}");

        if let Err(err) = server_init(&socket_path) {
            eprintln!(
                "[lintx] failed to start server on socket `{}`: {}",
                socket_path, err
            );
            if err.kind() == ErrorKind::PermissionDenied {
                eprintln!(
                    "[lintx] hint: in WSL, keep socket under `/tmp`, e.g. `export LINTX_SOCKET_PATH=/tmp/lintx-rpsocket`"
                );
            }
            std::process::exit(1);
        }
    } else {
        let mut passthrough_args = cli.other.unwrap_or_default();
        let mut detach = cli.detach;

        // Accept `LinTx -- --detach -- <module...>` as a user-friendly alias.
        // Proper form remains: `LinTx --detach -- <module...>`.
        if !detach
            && matches!(
                passthrough_args.first().map(|s| s.as_str()),
                Some("--detach")
            )
        {
            detach = true;
            passthrough_args.remove(0);
            if matches!(passthrough_args.first().map(|s| s.as_str()), Some("--")) {
                passthrough_args.remove(0);
            }
            debug_log("interpreted leading `--detach` in passthrough args as detach mode");
        }

        if matches!(passthrough_args.first().map(|s| s.as_str()), Some("--")) {
            passthrough_args.remove(0);
        }

        let cmd = passthrough_args.join(" ");
        if cmd.trim().is_empty() {
            eprintln!(
                "[lintx] no client command provided. Example: `LinTx -- ui_demo --backend sdl`"
            );
            std::process::exit(2);
        }
        let mut client = match Client::new(&socket_path) {
            Ok(c) => c,
            Err(err) => {
                eprintln!(
                    "[lintx] failed to connect to server socket `{}`: {}",
                    socket_path, err
                );
                eprintln!(
                    "[lintx] hint: start server first: `cargo run --features sdl_ui -- --server`"
                );
                std::process::exit(1);
            }
        };
        if detach {
            let detached = format!("__DETACH__ {cmd}");
            client.send_str(detached.as_str());
            return;
        }
        client.send_str(cmd.as_str());
        client.block_read();
    }
}

#[cfg(target_os = "windows")]
fn run_windows_local_mode() {
    let cli = Cli::parse();
    if cli.server {
        println!("Windows currently supports local mode only. Run: LinTx -- <module args>");
        return;
    }

    if let Some(other) = cli.other {
        execute_module_inline(&other);
    }
}

#[cfg(target_os = "windows")]
fn execute_module_inline(args: &[String]) {
    if args.is_empty() {
        return;
    }

    let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
    if let Some(module) = Module::try_get_module(argv[0]) {
        module.execute(argv.len() as u32, argv.as_ptr());
    } else {
        eprintln!("[lintx] unknown module `{}`", argv[0]);
        std::process::exit(2);
    }
}
