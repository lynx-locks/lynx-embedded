use anyhow::Result;
use core::borrow::Borrow;
use core::convert::Infallible;
use core::time::Duration;
use std::task::Poll;

use embedded_hal::spi::{Operation, MODE_0};

use embedded_hal_0_2::prelude::*;
use embedded_hal_0_2::timer::CountDown;
use esp_idf_svc::hal::gpio::{Gpio2, Gpio4, Output, OutputMode, Pin, PinDriver};
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};

use lynx_embedded::wifi as espWifi;

use esp_idf_svc::hal::spi::*;
use esp_idf_svc::hal::timer::{Timer, TimerConfig, TimerDriver};
use esp_idf_svc::sys::EspError;
use pn532::doc_test_helper::get_pn532;
use pn532::spi::{
    SPIInterface, PN532_SPI_DATAREAD, PN532_SPI_DATAWRITE, PN532_SPI_READY, PN532_SPI_STATREAD,
};
use pn532::IntoDuration;
use pn532::{requests::SAMMode, Interface, Pn532, Request};

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::spi::config::BitOrder;

struct SpiWrapper<'d, T>
where
    T: Borrow<SpiDriver<'d>> + 'd,
{
    device: SpiDeviceDriver<'d, T>,
    irq: PinDriver<'d, Gpio4, Output>,
}

impl<'d, T> SpiWrapper<'d, T>
where
    T: Borrow<SpiDriver<'d>> + 'd,
{
    fn wrap(device: SpiDeviceDriver<'d, T>, irq: PinDriver<'d, Gpio4, Output>) -> Self {
        Self { device, irq }
    }
}

impl<'d, T> Interface for SpiWrapper<'d, T>
where
    T: Borrow<SpiDriver<'d>> + 'd,
{
    type Error = EspError;

    fn write(&mut self, frame: &[u8]) -> std::result::Result<(), Self::Error> {
        self.device.transaction(&mut [
            Operation::Write(&[PN532_SPI_DATAWRITE]),
            Operation::Write(frame),
        ])
    }

    fn wait_ready(&mut self) -> Poll<std::result::Result<(), Self::Error>> {
        let mut buf = [0u8];
        self.device.transaction(&mut [
            Operation::Write(&[PN532_SPI_STATREAD]),
            Operation::Read(&mut buf),
        ])?;
        // self.device.write(&[PN532_SPI_STATREAD])?;
        // self.device.read(&mut buf)?;

        println!("{:?} {}", buf, PN532_SPI_READY);
        if buf[0] == PN532_SPI_READY {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
        // if self.irq.is_set_low() {
        //     Poll::Ready(Ok(()))
        // } else {
        //     Poll::Pending
        // }
    }

    fn read(&mut self, buf: &mut [u8]) -> std::result::Result<(), Self::Error> {
        self.device.transaction(&mut [
            Operation::Write(&[PN532_SPI_DATAREAD]),
            Operation::Read(buf),
        ])
    }
}

struct CsWrapper<'d, T: Pin, MODE: OutputMode> {
    cs: PinDriver<'d, T, MODE>,
}

impl<'d, T: Pin, MODE: OutputMode> embedded_hal_0_2::digital::v2::OutputPin
    for CsWrapper<'d, T, MODE>
{
    type Error = Infallible;

    fn set_low(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(self.cs.set_low().unwrap())
    }

    fn set_high(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(self.cs.set_high().unwrap())
    }
}

struct TIM<'d> {
    driver: TimerDriver<'d>,
    duration: Duration,
}

impl<'d> TIM<'d> {
    fn wrap(driver: TimerDriver<'d>) -> Self {
        Self {
            driver,
            duration: Duration::from_millis(0),
        }
    }
}

impl<'d> CountDown for TIM<'d> {
    type Time = Duration;

    fn start<T>(&mut self, duration: T)
    where
        T: Into<Self::Time>,
    {
        self.duration = duration.into();
        self.driver.set_counter(0).unwrap();
        self.driver.enable(true).unwrap();
    }

    fn wait(&mut self) -> nb::Result<(), void::Void> {
        let count = self.driver.counter().unwrap();
        // println!("{}", count);
        if count >= self.duration.as_millis() as u64 {
            return Ok(());
        }
        Err(nb::Error::WouldBlock)
    }
}

fn main() -> Result<()> {
    // Bind the log crate to the ESP Logging facilities
    EspLogger::initialize_default();

    // Configure Wifi
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    //
    // let mut wifi = BlockingWifi::wrap(
    //     EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
    //     sys_loop,
    // )?;
    //
    // espWifi::connect(&mut wifi)?;
    // log::info!("Wifi connected!");

    let spi = peripherals.spi2;

    let sclk = peripherals.pins.gpio6;
    let miso = peripherals.pins.gpio2; // SDI
    let mosi = peripherals.pins.gpio7; // SDO
    let cs = peripherals.pins.gpio10;

    let mut irq = PinDriver::output(peripherals.pins.gpio4)?;
    // irq.set_high()?;

    let timer_config = TimerConfig::new();
    let timer = TIM::wrap(TimerDriver::new(peripherals.timer10, &timer_config)?);

    let driver = SpiDriver::new::<SPI2>(spi, sclk, mosi, Some(miso), &SpiDriverConfig::new())?;
    //
    FreeRtos::delay_ms(500);
    let config = config::Config::new()
        .baudrate(1000000.Hz().into())
        .data_mode(MODE_0)
        .bit_order(BitOrder::LsbFirst);
    let mut device = SpiWrapper::wrap(SpiDeviceDriver::new(&driver, Some(cs), &config)?, irq);
    // let mut device = SpiDeviceDriver::new(&driver, Some(cs), &config)?;

    //
    // let config_2 = config::Config::new().baudrate(13.MHz().into());
    // let mut device_2 = SpiDeviceDriver::new(&driver, Some(cs_2), &config_2)?;

    // let interface = SPIInterface { spi: device_1, cs };

    let mut pn532: Pn532<_, _, 32> = Pn532::new(device, timer);
    loop {
        FreeRtos::delay_ms(500);
        if let Err(e) = pn532.process(
            &Request::GET_FIRMWARE_VERSION,
            4,
            Duration::from_millis(5000),
        ) {
            println!("Could not initialize PN532: {e:?}")
        }
    }
    // do something else
    // FreeRtos::delay_ms(500);

    // println!("Result {:?}", result);
    //

    ////////////////////////

    // if let Err(e) = pn532.process(
    //     &Request::sam_configuration(SAMMode::Normal, false),
    //     0,
    //     Duration::from_millis(5000),
    // ) {
    //     println!("Could not initialize PN532: {e:?}")
    // }
    // if let Ok(uid) = pn532.process(
    //     &Request::INLIST_ONE_ISO_A_TARGET,
    //     7,
    //     Duration::from_millis(1000),
    // ) {
    //     let result = pn532
    //         .process(&Request::ntag_read(10), 17, Duration::from_millis(50))
    //         .unwrap();
    //     if result[0] == 0x00 {
    //         println!("page 10: {:?}", &result[1..5]);
    //     }
    // }

    Ok(())
}
