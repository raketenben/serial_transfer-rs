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
#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(generic_const_exprs)]
use array_concat::*;

use core::marker::PhantomData;
#[cfg(feature = "std")]
use std::io::ErrorKind;

#[cfg(not(feature = "std"))]
type TransferError = embedded_io::ErrorKind;
#[cfg(feature = "std")]
type TransferError = std::io::Error;

#[cfg(not(feature = "std"))]
use core::mem::transmute_copy;
#[cfg(feature = "std")]
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

//use embedded_io::ErrorType;

//using the appropriate Read and Write trait based on the feature
#[cfg(not(feature = "std"))]
use embedded_io::{Read, Write};
#[cfg(feature = "std")]
use std::io::{Read, Write};

/// Trait for Read and Write
/// This allows us to accept any type that implements both Read and Write either from embedded_io or std::io
pub trait Stream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransferError>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, TransferError>;
    fn flush(&mut self) -> Result<(), TransferError>;
}

impl<T: Read + Write> Stream for T {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransferError> {
        match Read::read(self, buf) {
            Ok(count) => Ok(count),
            #[cfg(feature = "std")]
            Err(e) => Err(TransferError::new(ErrorKind::Other, e.to_string())),
            #[cfg(not(feature = "std"))]
            Err(_) => Err(TransferError::Other),
        }
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, TransferError> {
        match Write::write(self, buf) {
            Ok(count) => Ok(count),
            #[cfg(feature = "std")]
            Err(e) => Err(TransferError::new(ErrorKind::Other, e.to_string())),
            #[cfg(not(feature = "std"))]
            Err(_) => Err(TransferError::Other),
        }
    }

    fn flush(&mut self) -> Result<(), TransferError> {
        match Write::flush(self) {
            Ok(_) => Ok(()),
            #[cfg(feature = "std")]
            Err(e) => Err(TransferError::new(ErrorKind::Other, e.to_string())),
            #[cfg(not(feature = "std"))]
            Err(_) => Err(TransferError::Other),
        }
    }
}

pub struct SerialTransfer<P: Stream, T: Sized, const COUNT: usize> {
    read_write: P,
    next_token: NextToken,

    id_byte: u8,
    overhead_byte: u8,
    payload_length: usize,
    payload: [u8; COUNT],
    payload_index: usize,
    crc: CRC,

    phantom_type: PhantomData<T>,
}

#[cfg(all(feature = "serialport", feature = "std"))]
use serialport::SerialPort;
#[cfg(all(feature = "serialport", feature = "std"))]
impl<T: Sized, const COUNT: usize> From<Box<dyn SerialPort>>
    for SerialTransfer<Box<dyn SerialPort>, T, COUNT>
{
    fn from(serialport: Box<dyn SerialPort>) -> Self {
        SerialTransfer::new(serialport)
    }
}
/*
#[cfg(not(feature = "std"))]
impl<T : Sized, const COUNT: usize> From<avr_hal_generic::usart::Usart> for SerialTransfer<avr_hal_generic::usart::Usart, T, COUNT> {
    fn from(usart: avr_hal_generic::usart::Usart) -> Self {
        SerialTransfer::new(usart)
    }
}*/

impl<P: Stream, T: Sized, const COUNT: usize> SerialTransfer<P, T, COUNT> {
    pub fn new(read_write: P) -> SerialTransfer<P, T, COUNT> {
        SerialTransfer {
            next_token: NextToken::StartByte,
            read_write,
            id_byte: 0,
            overhead_byte: 0,
            payload_length: 0,
            payload: [0; COUNT],
            payload_index: 0,
            crc: CRC::new(0x9B),

            phantom_type: PhantomData,
        }
    }

    //Sum<COUNT, P2> : ArrayLength,

    /// Sends data over the serial port.
    /// Count is the size in bytes of the data to send.
    pub fn send(&mut self, data: T) -> Result<(), TransferError>
    where
        [(); COUNT + 3]:,
        [(); COUNT + 6]:,
    {
        let buffer: [u8; COUNT] = unsafe { transmute_copy(&data) };

        let packet_id = 0;

        //find first START_BYTE occurence in packet data
        let overflow_byte = match buffer.iter().position(|&x| x == START_BYTE) {
            Some(index) => (index) as u8,
            None => 0xFF,
        };

        //encode data with COBS
        let payload_cobs = self.encode_data_cobs(buffer);

        //make sure crc contains important data
        let preamble = [packet_id, overflow_byte, payload_cobs.len() as u8];
        //add packet header
        let crc_content: [u8; COUNT + 3] = concat_arrays!(preamble, payload_cobs);

        //calculate CRC (Error Detection Code)
        let crc = self.crc.calculate(&crc_content);

        let packet: [u8; COUNT + 6] = concat_arrays!([START_BYTE], crc_content, [crc, STOP_BYTE]);

        let _ = self.read_write.write(&packet)?;

        Ok(())
    }

    /// Checks if there is data available to read.
    /// If there is data available, it will return the data.
    pub fn available(&mut self) -> Result<Option<T>, TransferError>
    where
        [(); COUNT + 3]:,
    {
        loop {
            //show state and status in test only
            let mut byte: [u8; 1] = [0; 1];
            let read_bytes_count = self.read_write.read(&mut byte)?;
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
                    if byte[0] == COUNT as u8 {
                        self.payload_length = byte[0] as usize;
                        self.next_token = NextToken::Payload;
                        self.payload_index = 0;
                    } else {
                        self.next_token = NextToken::StartByte;
                    }
                }
                NextToken::Payload => {
                    self.payload[self.payload_index] = byte[0];
                    self.payload_index += 1;

                    if self.payload_index == self.payload_length {
                        self.next_token = NextToken::Crc;
                    } else {
                        self.next_token = NextToken::Payload;
                    }
                }
                NextToken::Crc => {
                    let crc_data: [u8; COUNT + 3] = concat_arrays!(
                        [self.id_byte, self.overhead_byte, self.payload_length as u8],
                        self.payload
                    );

                    let calculated_crc = self.crc.calculate(&crc_data);
                    let received_crc = byte[0];

                    //decode data with COBS
                    self.payload = self
                        .decode_data_cobs(self.payload, self.overhead_byte)
                        .into();

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
                        let dst: T = unsafe { transmute_copy(&self.payload) };
                        return Ok(Some(dst));
                    } else {
                        self.next_token = NextToken::StartByte;
                        return Ok(None);
                    }
                }
            }
        }

        Ok(None)
    }

    fn encode_data_cobs(&mut self, mut data: [u8; COUNT]) -> [u8; COUNT] {
        //find last byte
        let mut last_byte_index: Option<usize> = None;
        for i in (0..COUNT).rev() {
            if data[i] == START_BYTE {
                last_byte_index = Some(i);
                break;
            }
        }

        match last_byte_index {
            Some(index) => {
                let mut reference_index: u8 = index as u8;

                for i in (0..COUNT as u8).rev() {
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

    fn decode_data_cobs(&mut self, mut data: [u8; COUNT], overhead_byte: u8) -> [u8; COUNT] {
        let mut reference_index = overhead_byte;
        let mut overflowed;

        while reference_index < COUNT as u8 {
            let offset = data[reference_index as usize];
            data[reference_index as usize] = START_BYTE;
            (reference_index, overflowed) = reference_index.overflowing_add(offset);
            if overflowed {
                break;
            }
        }

        data
    }

    /// Blocks until all data has been written
    pub fn flush(&mut self) -> Result<(), TransferError> {
        self.read_write.flush()?;
        Ok(())
    }
}
