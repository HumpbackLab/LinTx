use rpos::thread_logln;

use linux_embedded_hal::I2cdev;
use std::time::Duration;

use crate::messages::{
    publish_input_frame, publish_input_status, AdcRawMsg, InputFrameMsg, InputHealth, InputSource,
    InputStatusMsg,
};
use ads1x1x::{channel, Ads1x1x, SlaveAddr};

const ADC_POLL_INTERVAL: Duration = Duration::from_micros(500);

fn read_channel<CH>(
    adc: &mut Ads1x1x<
        I2cdev,
        ads1x1x::ic::Ads1115,
        ads1x1x::ic::Resolution16Bit,
        ads1x1x::mode::OneShot,
    >,
    mut channel: impl FnMut() -> CH,
) -> i16
where
    CH: ads1x1x::ChannelId<
        Ads1x1x<I2cdev, ads1x1x::ic::Ads1115, ads1x1x::ic::Resolution16Bit, ads1x1x::mode::OneShot>,
    >,
{
    loop {
        match adc.read(channel()) {
            Ok(value) => return value >> 4,
            Err(nb::Error::WouldBlock) => std::thread::sleep(ADC_POLL_INTERVAL),
            Err(nb::Error::Other(err)) => panic!("adc read failed: {err:?}"),
        }
    }
}

fn adc_main(_argc: u32, _argv: *const &str) {
    let dev = I2cdev::new("/dev/i2c-0").unwrap();
    let address = SlaveAddr::default();
    let mut adc = Ads1x1x::new_ads1115(dev, address);
    adc.set_full_scale_range(ads1x1x::FullScaleRange::Within4_096V)
        .unwrap();
    adc.set_data_rate(ads1x1x::DataRate16Bit::Sps860).unwrap();

    let adc_raw_tx = rpos::msg::get_new_tx_of_message::<AdcRawMsg>("adc_raw").unwrap();
    let input_frame_tx = rpos::msg::get_new_tx_of_message::<InputFrameMsg>("input_frame").unwrap();
    let input_status_tx =
        rpos::msg::get_new_tx_of_message::<InputStatusMsg>("input_status").unwrap();

    thread_logln!("adc thread started!");
    publish_input_status(
        &input_status_tx,
        InputSource::Adc,
        InputHealth::Running,
        "/dev/i2c-0",
        4,
    );

    loop {
        let value = [
            read_channel(&mut adc, || channel::SingleA0),
            read_channel(&mut adc, || channel::SingleA1),
            read_channel(&mut adc, || channel::SingleA2),
            read_channel(&mut adc, || channel::SingleA3),
        ];

        publish_input_frame(&input_frame_tx, Some(&adc_raw_tx), InputSource::Adc, &value);
    }
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("adc", adc_main);
}
