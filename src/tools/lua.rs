use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use clap::Parser;
use crc::{Crc, CRC_8_DVB_S2};
use crsf::{PacketAddress, RawPacket};
use mlua::{
    Error as LuaError, Lua, Result as LuaResult, String as LuaString, Table, UserData,
    UserDataMethods,
};
use rpos::thread_logln;

use crate::client_process_args;

const CRC8_DVB_S2: Crc<u8> = Crc::<u8>::new(&CRC_8_DVB_S2);

#[derive(Parser)]
#[command(name = "lua_run", about = "Run a Lua script with UART/CRSF helpers")]
struct Cli {
    #[arg(value_name = "SCRIPT")]
    script: String,

    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(Clone)]
struct LuaSerialPort {
    inner: Arc<Mutex<Box<dyn serialport::SerialPort>>>,
}

impl LuaSerialPort {
    fn lock_port(&self) -> LuaResult<std::sync::MutexGuard<'_, Box<dyn serialport::SerialPort>>> {
        self.inner
            .lock()
            .map_err(|_| LuaError::external("serial port mutex poisoned"))
    }
}

impl UserData for LuaSerialPort {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("set_timeout", |_, this, timeout_ms: u64| {
            let mut port = this.lock_port()?;
            port.set_timeout(Duration::from_millis(timeout_ms))
                .map_err(LuaError::external)?;
            Ok(())
        });

        methods.add_method_mut("write", |_, this, data: LuaString| {
            let mut port = this.lock_port()?;
            port.write_all(data.as_bytes().as_ref())
                .map_err(LuaError::external)?;
            port.flush().map_err(LuaError::external)?;
            Ok(data.as_bytes().len())
        });

        methods.add_method_mut("write_hex", |_, this, hex: String| {
            let buf = decode_hex(&hex)?;
            let mut port = this.lock_port()?;
            port.write_all(&buf).map_err(LuaError::external)?;
            port.flush().map_err(LuaError::external)?;
            Ok(buf.len())
        });

        methods.add_method_mut(
            "read",
            |lua, this, (max_len, timeout_ms): (usize, Option<u64>)| {
                let mut port = this.lock_port()?;
                if let Some(timeout_ms) = timeout_ms {
                    port.set_timeout(Duration::from_millis(timeout_ms))
                        .map_err(LuaError::external)?;
                }

                let mut buf = vec![0u8; max_len];
                let len = match port.read(&mut buf) {
                    Ok(len) => len,
                    Err(err) if err.kind() == std::io::ErrorKind::TimedOut => 0,
                    Err(err) => return Err(LuaError::external(err)),
                };
                lua.create_string(&buf[..len])
            },
        );

        methods.add_method_mut(
            "read_hex",
            |_, this, (max_len, timeout_ms): (usize, Option<u64>)| {
                let mut port = this.lock_port()?;
                if let Some(timeout_ms) = timeout_ms {
                    port.set_timeout(Duration::from_millis(timeout_ms))
                        .map_err(LuaError::external)?;
                }

                let mut buf = vec![0u8; max_len];
                let len = match port.read(&mut buf) {
                    Ok(len) => len,
                    Err(err) if err.kind() == std::io::ErrorKind::TimedOut => 0,
                    Err(err) => return Err(LuaError::external(err)),
                };
                Ok(encode_hex(&buf[..len]))
            },
        );
    }
}

pub fn lua_run_main(argc: u32, argv: *const &str) {
    let Some(args) = client_process_args::<Cli>(argc, argv) else {
        return;
    };

    let script = match std::fs::read_to_string(&args.script) {
        Ok(content) => content,
        Err(err) => {
            thread_logln!("failed to read lua script {}: {}", args.script, err);
            return;
        }
    };

    let lua = Lua::new();
    if let Err(err) = install_lua_api(&lua, &args.script, &args.args) {
        thread_logln!("failed to install lua api: {}", err);
        return;
    }

    if let Err(err) = lua.load(&script).set_name(&args.script).exec() {
        thread_logln!("lua script failed: {}", err);
    }
}

fn install_lua_api(lua: &Lua, script_path: &str, args: &[String]) -> LuaResult<()> {
    let globals = lua.globals();
    globals.set("SCRIPT_PATH", script_path)?;

    let lua_args = lua.create_table()?;
    for (idx, value) in args.iter().enumerate() {
        lua_args.set(idx + 1, value.as_str())?;
    }
    globals.set("ARGS", lua_args)?;

    globals.set(
        "sleep_ms",
        lua.create_function(|_, timeout_ms: u64| {
            std::thread::sleep(Duration::from_millis(timeout_ms));
            Ok(())
        })?,
    )?;

    globals.set(
        "log",
        lua.create_function(|_, msg: String| {
            thread_logln!("[lua] {}", msg);
            Ok(())
        })?,
    )?;

    globals.set(
        "bytes_from_hex",
        lua.create_function(|lua, hex: String| {
            let buf = decode_hex(&hex)?;
            lua.create_string(&buf)
        })?,
    )?;

    globals.set(
        "hex",
        lua.create_function(|_, data: LuaString| Ok(encode_hex(data.as_bytes().as_ref())))?,
    )?;

    globals.set(
        "uart_open",
        lua.create_function(|_, (path, baudrate): (String, u32)| {
            let port = serialport::new(path, baudrate)
                .timeout(Duration::from_millis(100))
                .open()
                .map_err(LuaError::external)?;
            Ok(LuaSerialPort {
                inner: Arc::new(Mutex::new(port)),
            })
        })?,
    )?;

    let crsf = lua.create_table()?;
    crsf.set(
        "encode",
        lua.create_function(|lua, (dest, frame_type, payload): (u8, u8, LuaString)| {
            let packet = encode_crsf_packet(dest, frame_type, payload.as_bytes().as_ref());
            lua.create_string(&packet)
        })?,
    )?;
    crsf.set(
        "rc_channels",
        lua.create_function(|lua, channels: Table| {
            let packet = encode_crsf_rc_channels(channels)?;
            lua.create_string(packet.data())
        })?,
    )?;
    globals.set("crsf", crsf)?;

    Ok(())
}

fn encode_crsf_packet(dest: u8, frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 4);
    out.push(dest);
    out.push((payload.len() + 2) as u8);
    out.push(frame_type);
    out.extend_from_slice(payload);
    out.push(CRC8_DVB_S2.checksum(&out[2..]));
    out
}

fn encode_crsf_rc_channels(channels: Table) -> LuaResult<RawPacket> {
    let mut values = [crsf::RcChannels::CHANNEL_VALUE_MID; 16];
    for idx in 1..=16 {
        if let Ok(Some(value)) = channels.get::<Option<u16>>(idx) {
            values[idx - 1] = value.clamp(
                crsf::RcChannels::CHANNEL_VALUE_MIN,
                crsf::RcChannels::CHANNEL_VALUE_MAX,
            );
        }
    }
    Ok(crsf::Packet::RcChannels(crsf::RcChannels(values)).into_raw(PacketAddress::Transmitter))
}

fn decode_hex(hex: &str) -> LuaResult<Vec<u8>> {
    let compact: String = hex.chars().filter(|ch| !ch.is_ascii_whitespace()).collect();
    if !compact.len().is_multiple_of(2) {
        return Err(LuaError::external("hex string must have even length"));
    }

    let mut out = Vec::with_capacity(compact.len() / 2);
    for idx in (0..compact.len()).step_by(2) {
        let byte = u8::from_str_radix(&compact[idx..idx + 2], 16).map_err(|err| {
            LuaError::external(format!("invalid hex at byte {}: {}", idx / 2, err))
        })?;
        out.push(byte);
    }
    Ok(out)
}

fn encode_hex(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for byte in data {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0f));
    }
    out
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_hex, encode_crsf_packet, encode_hex};

    #[test]
    fn test_hex_roundtrip() {
        let raw = decode_hex("ee 06 2d ee").unwrap();
        assert_eq!(encode_hex(&raw), "ee062dee");
    }

    #[test]
    fn test_crsf_packet_layout() {
        let packet = encode_crsf_packet(0xee, 0x2d, &[0xee, 0xea, 0x01, 0x00]);
        assert_eq!(packet[0], 0xee);
        assert_eq!(packet[1], 6);
        assert_eq!(packet[2], 0x2d);
        assert_eq!(&packet[3..7], &[0xee, 0xea, 0x01, 0x00]);
        assert_eq!(packet.len(), 8);
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("lua_run", lua_run_main);
}
