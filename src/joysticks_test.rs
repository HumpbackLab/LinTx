use rpos::{msg::get_new_rx_of_message, thread_log};

use crate::mixer::MixerOutMsg;

fn channel_out(mixout: &MixerOutMsg) {
    thread_log!("\x1b[2KCH3 Thrust:{}\n", mixout.channels[2]);
    thread_log!("\x1b[2KCH4 Direction:{}\n", mixout.channels[3]);
    thread_log!("\x1b[2KCH1 Aileron:{}\n", mixout.channels[0]);
    thread_log!("\x1b[2KCH2 Elevator:{}\n", mixout.channels[1]);
    thread_log!("\x1b[4A");
}
fn joysticks_test_main(_argc: u32, _argv: *const &str) {
    let mut rx = get_new_rx_of_message::<MixerOutMsg>("mixer_out").unwrap();
    loop {
        channel_out(&rx.read());
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("joysticks_test", joysticks_test_main);
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn test_channel_out() {
        for i in 0..3 as u16 {
            let mixout = MixerOutMsg {
                channels: [i * 100; 16],
            };
            channel_out(&mixout);
        }
    }
}
