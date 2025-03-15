/*!
# SerialTransfer

SerialTransfer is a Rust library that allows you to send and receive data over a serial port.
This is a port of the [SerialTransfer](https://github.com/PowerBroker2/SerialTransfer) library for Arduino.

## Usage
Add the following to your Cargo.toml
```toml
[dependencies]
serialtransfer = "0.2"
```

Import the library
```rust
use serialtransfer::SerialTransfer;
```

Declaring a struct to send over the serial port
```rust
#[derive(Debug)]
struct Foo {
	bar: u8,
}
```

### Instantiation
Open a serial port and pass it to SerialTransfer
```no_run
let port = serialport::new("COM4", 9600).open().expect("Failed to open serial port");
let serial = SerialTransfer::from(port);
```

### Sending Data
Create an instance of the struct and send it over the serial port
```no_run
let mut foo = Foo { bar: 42 };
serial.send::<Foo, 1>(&foo).expect("Failed to send data");
```

### Receiving Data
and to recieve the data
```no_run
let received_foo = serial.available::<Foo, 1>().expect("Failed to receive data");
```
**_NOTE:_** You need to specify the size of the struct in bytes as a const parameter. (e.g. **<Foo, 1>** for a struct with a single byte field)
*/

use std::io::Error;
use std::io::{Read, Write};
use std::mem::transmute_copy;

mod crc;
mod tests;
use crc::CRC;

#[derive(Debug)]
enum NextToken {
    StartByte,
    IdByte,
    OverheadByte,
    PayloadLength,
    Payload,
    Crc,
    StopByte,
}

const START_BYTE: u8 = 0x7E;
const STOP_BYTE: u8 = 0x81;

const MAX_PACKET_SIZE: u8 = 0xFE;

/// Trait for Read and Write
/// This allows us to accept any type that implements both Read and Write
pub trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

pub struct SerialTransfer<P: ReadWrite> {
    crc: CRC,

    read_write: P,
    next_token: NextToken,

    id_byte: u8,
    overhead_byte: u8,
    payload_length: u8,
    payload: Vec<u8>,
}

#[cfg(feature = "serialport")]
use serialport::SerialPort;
impl From<Box<dyn SerialPort>> for SerialTransfer<Box<dyn SerialPort>> {
    fn from(serialport: Box<dyn SerialPort>) -> Self {
        SerialTransfer::new(serialport)
    }
}

impl<P: ReadWrite> SerialTransfer<P> {
    pub fn new(read_write: P) -> SerialTransfer<P> {
        println!("new");
        SerialTransfer {
            crc: CRC::new(0x9B),

            next_token: NextToken::StartByte,
            read_write,
            id_byte: 0,
            overhead_byte: 0,
            payload_length: 0,
            payload: Vec::new(),
        }
    }

    /// Sends data over the serial port.
    /// Count is the size in bytes of the data to send.
    pub fn send<T: Sized, const COUNT: usize>(&mut self, data: T) -> Result<(), Error> {
        let buffer: [u8; COUNT] = unsafe { transmute_copy(&data) };
        let buffer = buffer.to_vec();

        //find first START_BYTE occurence in packet data
        let overflow_byte = match buffer.iter().position(|&x| x == START_BYTE) {
            Some(index) => (index) as u8,
            None => 0xFF,
        };

        //encode data with COBS
        let buffer = self.encode_data_cobs(buffer);

        //calculate CRC (Error Detection Code)
        let crc = self.crc.calculate(&buffer, None);

        let mut packet: Vec<u8> = Vec::new();
        packet.push(START_BYTE);
        packet.push(0);
        packet.push(overflow_byte);
        packet.push(buffer.len() as u8);
        packet.append(&mut buffer.clone());
        packet.push(crc);
        packet.push(STOP_BYTE);

        let _ = self.read_write.write(&packet)?;

        Ok(())
    }

    /// Checks if there is data available to read.
    /// If there is data available, it will return the data.
    pub fn available<T: Sized, const COUNT: usize>(&mut self) -> Result<Option<T>, Error> {
        //while self.serialport.bytes_to_read()? > 0 {
        loop {
            //show state and status in test only
            let mut byte: [u8; 1] = [0; 1];
            let read_bytes_count = self.read_write.read(&mut byte)?;
            println!("byte: {:?}", byte);
            if read_bytes_count == 0 {
                break;
            };

            match self.next_token {
                NextToken::StartByte => {
                    if byte[0] == START_BYTE {
                        self.next_token = NextToken::IdByte;
                    }
                }
                NextToken::IdByte => {
                    self.id_byte = byte[0];
                    self.next_token = NextToken::OverheadByte;
                }
                NextToken::OverheadByte => {
                    self.overhead_byte = byte[0];
                    self.next_token = NextToken::PayloadLength;
                }
                NextToken::PayloadLength => {
                    if byte[0] > 0 && byte[0] < MAX_PACKET_SIZE {
                        self.payload_length = byte[0];
                        self.next_token = NextToken::Payload;
                        self.payload.clear();
                    } else {
                        self.next_token = NextToken::StartByte;
                    }
                }
                NextToken::Payload => {
                    if self.payload.len() < self.payload_length.into() {
                        self.payload.push(byte[0]);

                        if self.payload.len() == self.payload_length.into() {
                            self.next_token = NextToken::Crc;
                        } else {
                            self.next_token = NextToken::Payload;
                        }
                    }
                }
                NextToken::Crc => {
                    let calculated_crc =
                        self.crc.calculate(&self.payload, Some(self.payload_length));
                    let received_crc = byte[0];

                    //decode data with COBS
                    self.payload = self.decode_data_cobs(self.payload.clone(), self.overhead_byte);

                    if calculated_crc == received_crc {
                        self.next_token = NextToken::StopByte;
                    } else {
                        self.next_token = NextToken::StartByte;
                    }
                }
                NextToken::StopByte => {
                    self.next_token = NextToken::StartByte;

                    if byte[0] == STOP_BYTE {
                        self.next_token = NextToken::StartByte;
                        let buffer_conversion: Result<[u8; COUNT], Vec<u8>> =
                            self.payload.clone().try_into();

                        match buffer_conversion {
                            Ok(buffer) => {
                                let dst: T = unsafe { transmute_copy(&buffer) };
                                return Ok(Some(dst));
                            }
                            Err(_) => {
                                return Ok(None);
                            }
                        }
                    } else {
                        self.next_token = NextToken::StartByte;
                        return Ok(None);
                    }
                }
            }
        }

        Ok(None)
    }

    fn encode_data_cobs(&mut self, mut data: Vec<u8>) -> Vec<u8> {
        //find last byte
        let mut last_byte_index: Option<usize> = None;
        for i in (0..data.len()).rev() {
            if data[i] == START_BYTE {
                last_byte_index = Some(i);
                break;
            }
        }

        match last_byte_index {
            Some(index) => {
                let mut reference_index: u8 = index as u8;

                for i in (0..data.len() as u8).rev() {
                    if data[i as usize] == START_BYTE {
                        let (new_reference_index, _overflowed) = reference_index.overflowing_sub(i);
                        data[i as usize] = new_reference_index;
                        reference_index = i;
                    }
                }

                data
            }
            None => data,
        }
    }

    fn decode_data_cobs(&mut self, mut data: Vec<u8>, overhead_byte: u8) -> Vec<u8> {
        let mut reference_index = overhead_byte;
        let mut overflowed;

        while reference_index < data.len() as u8 {
            let offset = data[reference_index as usize];
            data[reference_index as usize] = START_BYTE;
            (reference_index, overflowed) = reference_index.overflowing_add(offset);
            if overflowed {
                break;
            }
        }

        data
    }

    /// Flushes the write buffer of the serial port.
    /// Careful: This function is blocking and will wait until all data has been written.
    pub fn flush(&mut self) -> Result<(), Error> {
        self.read_write.flush()?;
        Ok(())
    }
}
