#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

use csv::StringRecord;
use serde::Deserialize;
use serialport::{ClearBuffer, SerialPortInfo, SerialPortType};
use std::io::BufRead;
use std::io::BufReader;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Record {
    acc_x: f32,
    acc_y: f32,
    acc_z: f32,
    mag_x: f32,
    mag_y: f32,
    mag_z: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
}

fn main() {
    let port_info = find_usb_serial_port(0x04b9, 0x0010).expect("Failed to find port");

    let port = serialport::new(&port_info.port_name, 115_200)
        .timeout(Duration::from_millis(100))
        .open();

    match port {
        Ok(port) => {
            println!("Receiving data from {}", &port_info.port_name);

            //clear any data in the buffers
            port.clear(ClearBuffer::All)
                .expect("Failed to clear port buffers");

            //read and discard the first new line of data - could be incomplete
            let mut discard = String::new();
            let mut serial_reader = BufReader::new(port);
            serial_reader
                .read_line(&mut discard)
                .expect("Failed to read first line of serial data");

            let mut csv_reader = csv::Reader::from_reader(serial_reader);

            let mut r = StringRecord::new();

            loop {
                if csv_reader
                    .read_record(&mut r)
                    .expect("Failed to read CSV record")
                {
                    let rec: Record = r.deserialize(None).expect("Failed to deserialise record");
                    println!("{:?}", rec);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open \"{}\". Error: {}", &port_info.port_name, e);
            ::std::process::exit(1);
        }
    }
}

#[allow(clippy::similar_names)]
fn find_usb_serial_port(vid: u16, pid: u16) -> Option<SerialPortInfo> {
    serialport::available_ports()
        .map(|pv| {
            pv.into_iter().find(|p| match &p.port_type {
                SerialPortType::UsbPort(u) => u.vid == vid && u.pid == pid,
                SerialPortType::PciPort
                | SerialPortType::BluetoothPort
                | SerialPortType::Unknown => false,
            })
        })
        .expect("Failed to list available serial ports")
}
