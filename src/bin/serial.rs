#![no_std]
#![no_main]
#![warn(clippy::pedantic, clippy::nursery)]

use ahrs::{Ahrs, Madgwick};
use bsp::entry;
use bsp::hal;
use core::f32::consts::PI;
use core::fmt::Write;
use defmt::{error, info};
use defmt_rtt as _;
use embedded_hal::blocking::i2c;
use embedded_hal::blocking::i2c::SevenBitAddress;
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::digital::v2::ToggleableOutputPin;
use embedded_hal::timer::CountDown;
use fugit::ExtU32;
use fugit::RateExtU32;
use hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};
use nalgebra::{UnitQuaternion, Vector3};
use num_traits::ops::euclid::Euclid;
use panic_probe as _;
use rp_pico as bsp;
#[allow(clippy::wildcard_imports)]
use usb_device::{class_prelude::*, prelude::*};
use usbd_serial::SerialPort;

const IMU_ADDR: SevenBitAddress = 0x68;
const MAG_ADDR: SevenBitAddress = 0x0c;

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

    let imu_id = imu_who_am_i(&mut i2c).unwrap();
    assert_eq!(
        imu_id, 0xEA,
        "Unexpected i2c device id {:X}, expected 0xEA",
        imu_id
    );

    imu_enable_i2c_bypass(&mut i2c).unwrap();

    let mag_id = mag_who_am_i(&mut i2c).unwrap();
    assert_eq!(
        mag_id, 0x0948,
        "Unexpected i2c device id {:X}, expected 0xEA",
        mag_id
    );

    imu_wake(&mut i2c).unwrap();

    mag_wake(&mut i2c).unwrap();

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

    loop {
        // A welcome message at the beginning
        if log_count_down.wait().is_ok() {
            let m = mag_read(&mut i2c);
            let r = imu_read(&mut i2c);
            if m.is_err() || r.is_err() {
                continue;
            }
            let (gyro, acc) = r.unwrap();
            let mag = m.unwrap();

            let quat = ahrs.update(&gyro, &acc, &mag).unwrap();

            write_to_serial(&mut serial, &mut led_pin, acc, quat);
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
    quat: &UnitQuaternion<f32>,
) {
    let (roll, pitch, yaw) = quat.euler_angles();

    let deg = 360.0f32;

    let mut s = heapless::String::<256>::new();
    core::write!(
        &mut s,
        "/*{},{},{},{},{},{}*/\r\n",
        acc.x,
        acc.y,
        acc.y,
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

fn imu_who_am_i<I: i2c::WriteRead>(i2c: &mut I) -> Result<u8, I::Error> {
    let mut buffer = [1];
    //who am i?
    i2c.write_read(IMU_ADDR, &[0u8], &mut buffer)?;
    //expect EA
    info!("ID: {:X}", buffer);
    Ok(buffer[0])
}

fn mag_who_am_i<I: i2c::WriteRead>(i2c: &mut I) -> Result<u16, I::Error> {
    let mut buffer = [0; 2];
    //who am i?
    i2c.write_read(MAG_ADDR, &[0u8], &mut buffer)?;
    //expect EA
    info!("ID: {:X}", buffer);
    Ok(u16::from_le_bytes([buffer[0], buffer[1]]))
}

fn imu_enable_i2c_bypass<I: i2c::Write>(i2c: &mut I) -> Result<(), I::Error> {
    //Enable BYPASS_EN
    i2c.write(IMU_ADDR, &[0xF, 0x02])
}

fn imu_wake<I: i2c::Write>(i2c: &mut I) -> Result<(), I::Error> {
    //wake from sleep
    i2c.write(IMU_ADDR, &[0x6, 0x1])
}

fn mag_wake<I: i2c::Write>(i2c: &mut I) -> Result<(), I::Error> {
    //enable 100hz read
    i2c.write(MAG_ADDR, &[0x31, 0x8])
}

fn imu_read<I: i2c::WriteRead>(i2c: &mut I) -> Result<(Vector3<f32>, Vector3<f32>), I::Error> {
    let mut buffer = [0; 12];

    i2c.write_read(IMU_ADDR, &[0x2Du8], &mut buffer)?;

    let acc_x = f32::from(i16::from_be_bytes([buffer[0], buffer[1]]));
    let acc_y = f32::from(i16::from_be_bytes([buffer[2], buffer[3]]));
    let acc_z = f32::from(i16::from_be_bytes([buffer[4], buffer[5]]));

    let gyr_x = f32::from(i16::from_be_bytes([buffer[6], buffer[7]]));
    let gyr_y = f32::from(i16::from_be_bytes([buffer[8], buffer[9]]));
    let gyr_z = f32::from(i16::from_be_bytes([buffer[10], buffer[11]]));

    let gyro = Vector3::new(gyr_x, gyr_y, gyr_z) * (PI / 180.0) / 131.0;

    let acc = Vector3::new(acc_x, acc_y, acc_z) / 16384.0;
    Ok((gyro, acc))
}

fn mag_read<I: i2c::WriteRead>(i2c: &mut I) -> Result<Vector3<f32>, I::Error> {
    let mut buffer = [0; 9];

    i2c.write_read(MAG_ADDR, &[0x10], &mut buffer)?;

    //let status1 = buffer[0];

    let mag_x = f32::from(i16::from_le_bytes([buffer[1], buffer[2]]));
    let mag_y = f32::from(i16::from_le_bytes([buffer[3], buffer[4]]));
    let mag_z = f32::from(i16::from_le_bytes([buffer[5], buffer[6]]));

    //buffer[7] is a dummy register

    //let status2 = buffer[8];

    let mag = Vector3::new(mag_x, mag_y, mag_z);
    Ok(mag)
}
