#![no_std]
#![no_main]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

use ahrs::{Ahrs, Madgwick};
use bsp::entry;
use bsp::hal;
use core::fmt::Write;
use defmt::{error, info};
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::digital::v2::ToggleableOutputPin;
use embedded_hal::timer::CountDown;
use fugit::ExtU32;
use fugit::RateExtU32;
use hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};
use imu_playground::Imc20948;
use nalgebra::{UnitQuaternion, Vector3};
use num_traits::ops::euclid::Euclid;
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

    let i2c_master = hal::I2C::i2c1(
        pac.I2C1,
        sda_pin,
        scl_pin,
        400.kHz(),
        &mut pac.RESETS,
        &clocks.peripheral_clock,
    );

    let mut imc = Imc20948::new(i2c_master);

    imc.startup().unwrap();

    imc.imu_enable_i2c_bypass().unwrap();

    let mag_id = imc.mag_who_am_i().unwrap();
    assert_eq!(
        mag_id, 0x0948,
        "Unexpected i2c device id {:X}, expected 0xEA",
        mag_id
    );

    imc.imu_wake().unwrap();

    imc.mag_wake().unwrap();

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

    let mut ahrs = Madgwick::<f32>::new(0.1, 0.1);

    let mut n = 0;
    loop {
        // A welcome message at the beginning
        if log_count_down.wait().is_ok() {
            let m = imc.mag_read();
            let r = imc.imu_read();
            if m.is_err() || r.is_err() {
                continue;
            }
            let (gyro, acc) = r.unwrap();
            let rm = m.unwrap();

            n += 1;
            if n > 20 {
                info!(
                    "acc: {},{},{}, mag: {},{},{}",
                    acc.x, acc.y, acc.z, rm.x, rm.y, rm.z
                );
                n = 0;
            }

            let quat = ahrs.update(&gyro, &acc, &rm).unwrap();

            write_to_serial(&mut serial, &mut led_pin, acc, rm, quat);
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

fn write_to_serial<U: UsbBus, P: ToggleableOutputPin + OutputPin>(
    serial: &mut SerialPort<U>,
    led_pin: &mut P,
    acc: Vector3<f32>,
    mag: Vector3<f32>,
    quat: &UnitQuaternion<f32>,
) {
    let (roll, pitch, yaw) = quat.euler_angles();

    let deg = 360.0f32;

    let mut s = heapless::String::<256>::new();
    core::write!(
        &mut s,
        "{},{},{},{},{},{},{},{},{}\r\n",
        acc.x,
        acc.y,
        acc.y,
        mag.x,
        mag.y,
        mag.z,
        (roll.to_degrees()).rem_euclid(&deg),
        (pitch.to_degrees()).rem_euclid(&deg),
        (yaw.to_degrees()).rem_euclid(&deg)
    )
    .unwrap();

    if serial.write(s.as_bytes()).ok().is_some() {
        led_pin.toggle().ok();
    } else {
        led_pin.set_low().ok();
    }
}
