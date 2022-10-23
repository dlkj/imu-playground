#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

use serialport::{ClearBuffer, SerialPortInfo, SerialPortType};
use std::io;
use std::io::Write;
use std::time::Duration;

fn main() {
    let port_info = find_usb_serial_port(0x04b9, 0x0010).expect("Failed to find port");

    let port = serialport::new(&port_info.port_name, 115_200)
        .timeout(Duration::from_millis(10))
        .open();

    match port {
        Ok(mut port) => {
            port.clear(ClearBuffer::All)
                .expect("Failed to clear port buffers");
            let mut serial_buf: Vec<u8> = vec![0; 1000];
            println!("Receiving data from {}", &port_info.port_name);
            loop {
                match port.read(serial_buf.as_mut_slice()) {
                    Ok(t) => io::stdout().write_all(&serial_buf[..t]).unwrap(),
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                    Err(e) => eprintln!("{:?}", e),
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
