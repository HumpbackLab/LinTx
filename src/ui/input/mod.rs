mod events;

pub mod emit;
#[cfg(target_os = "linux")]
pub mod fifo;
#[cfg(target_os = "linux")]
pub mod key;

pub use events::*;
