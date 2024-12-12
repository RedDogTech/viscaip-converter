use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use clap::Parser;
use log::info;
use std::io::Cursor;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio_serial::SerialPortBuilderExt;

// const VISCA_ACK: [u8; 2] = [0x90, 0x41];
// const VISCA_COMP: [u8; 2] = [0x90, 0x51];
// const COMMAND: [u8; 2] = [0x01, 0x00];
// const CONTROL: [u8; 2] = [0x02, 0x00];
// const INQUIRY: [u8; 2] = [0x01, 0x10];
const REPLY: [u8; 2] = [0x01, 0x11];

#[derive(Parser, Debug)]
struct Args {
    /// Uplink Address (IP:Port)
    #[arg(short, long)]
    listen_address: String,

    /// Serial device
    #[arg(short, long)]
    serial_device: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let mut seq_number: u32 = 0;

    let listen_address: SocketAddr = args
        .listen_address
        .parse()
        .expect("Failed to parse listen address");

    let socket = UdpSocket::bind(listen_address).await?;

    info!("UDP Listen on:  {}", args.listen_address);
    info!("Serial radio on: {}", args.serial_device);

    let (serial_queue_writer, serial_queue_reader): (Sender<Vec<u8>>, Receiver<Vec<u8>>) =
        mpsc::channel();

    let mut serial_stream = tokio_serial::new(args.serial_device, 9600).open_native_async()?;

    serial_stream
        .set_exclusive(false)
        .expect("Failed to set serial to exclusive");

    let wrapped_serial = Arc::new(Mutex::new(serial_stream));
    let mut buf = vec![0; 1024];
    let thread_serial = Arc::clone(&wrapped_serial);

    thread::spawn(move || loop {
        if let Ok(data) = serial_queue_reader.recv() {
            let mut ser = thread_serial.lock().unwrap();
            let _ = ser.write(&data).unwrap();
        }
        thread::sleep(Duration::from_millis(250));
    });

    let main_serial = Arc::clone(&wrapped_serial);

    loop {
        socket.readable().await?;

        let (len, addr) = socket.recv_from(&mut buf).await?;

        if len > 0 {
            let visca_ip = &buf[..len];
            let (ip_header, rs232) = visca_ip.split_at(8);

            info!("-> udp header:{:02X?}, payload{:02X?}", ip_header, rs232);

            let mut header = Cursor::new(ip_header);

            header.read_u8()?;
            header.read_u8()?;
            header.read_u16::<BigEndian>()?;
            seq_number = header.read_u32::<BigEndian>()?;

            serial_queue_writer
                .send(rs232.to_vec())
                .expect("Failed to send??");
        }

        let len = {
            let mut ser = main_serial.lock().unwrap();

            ser.read(&mut buf)
        };
        if let Ok(serial_len) = len {
            if serial_len > 0 {
                let pieces: Vec<_> = buf[..serial_len]
                    .split(|&e| e == 0xff)
                    .filter(|v| !v.is_empty())
                    .collect();

                for payload in pieces {
                    info!("serial recived {:02X?}", &payload);

                    let mut res: Vec<u8> = Vec::new();

                    res.write(&REPLY)?;
                    res.write_u16::<BigEndian>((payload.len() as u16) + 1)?;
                    res.write_u32::<BigEndian>(seq_number)?;
                    res.write(payload)?;
                    res.write_u8(0xFF)?;

                    socket.send_to(&res, addr).await?;
                    info!("<- udp {:02X?} to {}", res, addr);
                }
            }
        }
    }
}
