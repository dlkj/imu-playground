#![no_std]
#![no_main]

use bsp::entry;
use bsp::hal;
use cortex_m::prelude::*;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use embedded_time::fixed_point::FixedPoint;
use embedded_time::rate::Kilohertz;
use hal::{
    clocks::{init_clocks_and_plls, Clock},
    pac,
    sio::Sio,
    watchdog::Watchdog,
};
use panic_probe as _;
use rp_pico as bsp;

#[entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

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

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());

    let pins = bsp::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let sda_pin = pins.gpio14.into_mode::<hal::gpio::FunctionI2C>();
    let scl_pin = pins.gpio15.into_mode::<hal::gpio::FunctionI2C>();

    let mut i2c = hal::I2C::i2c1(
        pac.I2C1,
        sda_pin,
        scl_pin,
        Kilohertz(400),
        &mut pac.RESETS,
        clocks.peripheral_clock,
    );

    {
        let mut buffer = [1];
        //who am i?
        i2c.write_read(0x68, &[0u8], &mut buffer).unwrap();
        //expect EA
        info!("ID: {:X}", buffer);
        if buffer[0] != 0xEA {
            crate::panic!("Unexpected i2c device id {:X}, expected 0xEA", buffer[0])
        }
    }

    //wake from sleep
    i2c.write(0x68, &[0x6, 0x1]).unwrap();

    //sensors take aprox ~30ms to start-up
    delay.delay_ms(50);

    let mut led_pin = pins.led.into_push_pull_output();

    loop {
        led_pin.set_high().unwrap();

        let mut b = [0; 12];

        //ACCEL
        i2c.write_read(0x68, &[0x2Du8], &mut b).unwrap();
        info!(
            "ACC: x:{:05}, y:{:05}, z:{:05}\nGYR: x:{:05}, y:{:05}, z:{:05}",
            i16::from_be_bytes([b[0], b[1]]),
            i16::from_be_bytes([b[2], b[3]]),
            i16::from_be_bytes([b[4], b[5]]),
            i16::from_be_bytes([b[6], b[7]]),
            i16::from_be_bytes([b[8], b[9]]),
            i16::from_be_bytes([b[10], b[11]]),
        );

        delay.delay_ms(500);
        led_pin.set_low().unwrap();
        delay.delay_ms(500);
    }
}
