use std::thread;
use std::time::Duration;

use anyhow::Result;

use esp_idf_svc::log::EspLogger;

use lynx_embedded::Led;

fn main() -> Result<()> {
    // Bind the log crate to the ESP Logging facilities
    EspLogger::initialize_default();

    //LED
    log::info!("Let's use the esp32c3 rgb led!");

    let mut led = Led::new(
        esp_idf_svc::sys::rmt_channel_t_RMT_CHANNEL_0,
        esp_idf_svc::sys::gpio_num_t_GPIO_NUM_8,
    )?;

    log::info!("Setting 1");
    led.set_color(0x10, 0x00, 0x00)?;
    thread::sleep(Duration::from_millis(1000));
    log::info!("Setting 2");
    led.set_color(0x00, 0x10, 0x00)?;
    thread::sleep(Duration::from_millis(1000));
    log::info!("Setting 3");
    led.set_color(0x00, 0x00, 0x10)?;
    thread::sleep(Duration::from_millis(1000));

    const L: u8 = 0x00;
    const H: u8 = 0x05;
    const DURATION: Duration = Duration::from_millis(10);
    const NUM_STEPS: u32 = 20;
    led.fade_to(L, L, L, 5, DURATION)?;
    loop {
        led.fade_to(H, L, L, NUM_STEPS, DURATION)?;
        led.fade_to(H, H, L, NUM_STEPS, DURATION)?;
        led.fade_to(L, H, L, NUM_STEPS, DURATION)?;
        led.fade_to(L, H, H, NUM_STEPS, DURATION)?;
        led.fade_to(L, L, H, NUM_STEPS, DURATION)?;
        led.fade_to(H, L, H, NUM_STEPS, DURATION)?;
    }
}
