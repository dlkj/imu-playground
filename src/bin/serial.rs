#![no_std]
#![no_main]
#![warn(clippy::pedantic, clippy::nursery)]

use bsp::entry;
use bsp::hal;
use core::fmt::Write;
use cortex_m::prelude::*;
use defmt::{error, info};
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::digital::v2::ToggleableOutputPin;
use embedded_hal::timer::CountDown;
use fugit::ExtU32;
use fugit::RateExtU32;
use hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};
use panic_probe as _;
use rp_pico as bsp;
#[allow(clippy::wildcard_imports)]
use usb_device::{class_prelude::*, prelude::*};
use usbd_serial::SerialPort;

#[entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    // let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

    let clocks = init_clocks_and_plls(
        bsp::XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let pins = {
        let sio = Sio::new(pac.SIO);

        bsp::Pins::new(
            pac.IO_BANK0,
            pac.PADS_BANK0,
            sio.gpio_bank0,
            &mut pac.RESETS,
        )
    };

    let sda_pin = pins.gpio14.into_mode::<hal::gpio::FunctionI2C>();
    let scl_pin = pins.gpio15.into_mode::<hal::gpio::FunctionI2C>();

    let mut i2c = hal::I2C::i2c1(
        pac.I2C1,
        sda_pin,
        scl_pin,
        400.kHz(),
        &mut pac.RESETS,
        &clocks.peripheral_clock,
    );

    {
        let mut buffer = [1];
        //who am i?
        i2c.write_read(0x68, &[0u8], &mut buffer).unwrap();
        //expect EA
        info!("ID: {:X}", buffer);
        assert!(
            buffer[0] == 0xEA,
            "Unexpected i2c device id {:X}, expected 0xEA",
            buffer[0]
        );
    }

    //wake from sleep
    i2c.write(0x68, &[0x6, 0x1]).unwrap();

    let usb_alloc = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut serial = SerialPort::new(&usb_alloc);

    let mut usb_dev = UsbDeviceBuilder::new(&usb_alloc, UsbVidPid(1209, 0x0010))
        .manufacturer("DLKJ")
        .product("Serial port IMU Playground")
        .serial_number("TEST")
        .device_class(2) // from: https://www.usb.org/defined-class-codes
        .build();

    let mut led_pin = pins.led.into_push_pull_output();

    let timer = hal::Timer::new(pac.TIMER, &mut pac.RESETS);

    let mut log_count_down = timer.count_down();
    log_count_down.start(100.millis());

    loop {
        // A welcome message at the beginning
        if log_count_down.wait().is_ok() {
            let mut s = heapless::String::<64>::new();

            let mut buffer = [0; 12];

            i2c.write_read(0x68, &[0x2Du8], &mut buffer).unwrap();

            core::write!(
                &mut s,
                "/*{},{},{},{},{},{}*/\n",
                i16::from_be_bytes([buffer[0], buffer[1]]),
                i16::from_be_bytes([buffer[2], buffer[3]]),
                i16::from_be_bytes([buffer[4], buffer[5]]),
                i16::from_be_bytes([buffer[6], buffer[7]]),
                i16::from_be_bytes([buffer[8], buffer[9]]),
                i16::from_be_bytes([buffer[10], buffer[11]])
            )
            .unwrap();

            if serial.write(s.as_bytes()).ok().is_some() {
                led_pin.toggle().unwrap();
            } else {
                led_pin.set_low().unwrap();
            }
        }

        // Check for new data
        if usb_dev.poll(&mut [&mut serial]) {
            let mut buf = [0u8; 64];
            match serial.read(&mut buf) {
                Err(UsbError::WouldBlock) | Ok(_) => {
                    // Do nothing
                }
                Err(e) => error!("serial read error: {}", e),
            }
        }
    }
}
