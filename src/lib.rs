#![no_std]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

use core::{f32::consts::PI, fmt::Debug};
use defmt::info;
use embedded_hal::blocking::i2c;
use nalgebra::Vector3;

const MAG_ADDR: i2c::SevenBitAddress = 0x0c;
const IMU_ADDR: i2c::SevenBitAddress = 0x68;

pub struct Imc20948<I, E>
where
    I: i2c::Read<Error = E> + i2c::Write<Error = E> + i2c::WriteRead<Error = E>,
{
    i2c: I,
}

#[derive(Debug)]
pub enum ImcError<E> {
    I2c(E),
    BadId,
}

impl<I, E> Imc20948<I, E>
where
    I: i2c::Read<Error = E> + i2c::Write<Error = E> + i2c::WriteRead<Error = E>,
{
    pub const fn new(i2c: I) -> Self {
        Self { i2c }
    }

    pub fn startup(&mut self) -> Result<(), ImcError<E>> {
        //check id
        let imu_id = self.imu_who_am_i().map_err(|e| ImcError::I2c(e))?;
        if imu_id != 0xEA {
            return Err(ImcError::BadId);
        }

        //set bank0
        self.imu_set_bank(0).map_err(|e| ImcError::I2c(e))?;

        //soft reset
        self.imu_soft_reset().map_err(|e| ImcError::I2c(e))?;

        //wake
        self.imu_wake().map_err(|e| ImcError::I2c(e))?;

        //full power

        //mag startup

        //non minimal stuff
        //sample mode

        //set scales

        Ok(())
    }

    pub fn imu_who_am_i(&mut self) -> Result<u8, E> {
        let mut buffer = [1];
        //who am i?
        self.i2c.write_read(IMU_ADDR, &[0u8], &mut buffer)?;
        //expect EA
        info!("ID: {:X}", buffer);
        Ok(buffer[0])
    }

    pub fn imu_enable_i2c_bypass(&mut self) -> Result<(), E> {
        //reset i2c master
        self.i2c.write(IMU_ADDR, &[0x3, 0x02])?;

        //Enable BYPASS_EN
        self.i2c.write(IMU_ADDR, &[0xF, 0x02])
    }

    pub fn imu_wake(&mut self) -> Result<(), E> {
        //wake from sleep
        self.i2c.write(IMU_ADDR, &[0x6, 0x1])
    }

    pub fn imu_read(&mut self) -> Result<(Vector3<f32>, Vector3<f32>), E> {
        let mut buffer = [0; 12];

        self.i2c.write_read(IMU_ADDR, &[0x2Du8], &mut buffer)?;

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

    pub fn mag_read(&mut self) -> Result<Vector3<f32>, E> {
        let mut buffer = [0; 9];

        self.i2c.write_read(MAG_ADDR, &[0x10], &mut buffer)?;

        //let status1 = buffer[0];

        //realign magnetometer axis with imu
        let mag_x = f32::from(i16::from_le_bytes([buffer[1], buffer[2]]));
        let mag_y = f32::from(i16::from_le_bytes([buffer[3], buffer[4]]));
        let mag_z = f32::from(i16::from_le_bytes([buffer[5], buffer[6]]));

        //buffer[7] is a dummy register

        //let status2 = buffer[8];

        Ok(Vector3::new(mag_x, mag_y, mag_z))
    }

    pub fn mag_who_am_i(&mut self) -> Result<u16, E> {
        let mut buffer = [0; 2];
        //who am i?
        self.i2c.write_read(MAG_ADDR, &[0u8], &mut buffer)?;
        //expect EA
        info!("ID: {:X}", buffer);
        Ok(u16::from_le_bytes([buffer[0], buffer[1]]))
    }

    pub fn mag_wake(&mut self) -> Result<(), E> {
        //enable 100hz read
        self.i2c.write(MAG_ADDR, &[0x31, 0x8])
    }

    fn imu_soft_reset(&mut self) -> Result<(), E> {
        let mut buffer = [0; 1];
        self.i2c.write_read(IMU_ADDR, &[0x06], &mut buffer)?;

        buffer[0] |= 0x01;

        self.i2c.write(IMU_ADDR, &[0x06, 0x01])
    }
    fn imu_set_bank(&mut self, bank: u8) -> Result<(), E> {
        //error if bank > 3

        let bank = (bank << 4) & 0x30;
        self.i2c.write(IMU_ADDR, &[0x7F, bank])
    }
}
