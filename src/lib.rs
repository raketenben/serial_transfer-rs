use std::io::Read;
use std::mem::transmute_copy;
use serialport::{Error, SerialPort};

mod crc;
use crc::CRC;

enum TransferStatus {
	Continue = 3,
	NewData = 2,
	NoData = 1,
	CrcError = 0,
	PayloadError = -1,
	StopByteError = -2,
}

enum TransferState {
	FindStartByte = 0,
	FindIdByte = 1,
	FindOverheadByte = 2,
	FindPayloadLength = 3,
	FindPayload = 4,
	FindCrc = 5,
	FindStopByte = 6,
}

const START_BYTE : u8 = 0x7E;
const STOP_BYTE : u8 = 0x81;

const MAX_PACKET_SIZE : u8 = 0xFE;



pub struct SerialTransfer {
	crc : CRC,

	serialport : Box<dyn SerialPort>,
	status : TransferStatus,
	transfer_state : TransferState,

	id_byte : u8,
	overhead_byte : u8,
	payload_length : u8,
	payload : Vec<u8>,
}

impl SerialTransfer {

	pub fn new(port : Box<dyn SerialPort>) -> SerialTransfer {
		SerialTransfer {
			crc : CRC::new(0x9B),

			status : TransferStatus::Continue,
			transfer_state : TransferState::FindStartByte,
			serialport : port,

			id_byte: 0,
			overhead_byte: 0,
			payload_length: 0,
			payload: Vec::new(),
		}
	}

	pub fn send<T : Sized, const COUNT: usize>(&mut self, data : T) -> Result<(),Error> {
		let buffer : [u8;COUNT] = unsafe { transmute_copy(&data) };
		let buffer = buffer.to_vec();

		let crc = self.crc.calculate(&buffer,None);

		let mut packet : Vec<u8> = Vec::new();
		packet.push(START_BYTE);
		packet.push(0);
		packet.push(255);
		packet.push(buffer.len() as u8);
		packet.append(&mut buffer.clone());
		packet.push(crc);
		packet.push(STOP_BYTE);

		self.serialport.write(&packet)?;
		self.serialport.flush()?;

		Ok(())
	}

	pub fn available<T : Sized, const COUNT: usize>(&mut self) -> Result<Option<T>,Error> {

		while self.serialport.bytes_to_read()? > 0 {
			let mut byte : [u8;1] = [0;1];
			self.serialport.read(&mut byte)?;

	
			match self.transfer_state {
				TransferState::FindStartByte => {
					if byte[0] == START_BYTE {
						self.transfer_state = TransferState::FindIdByte; 
					}
				},
				TransferState::FindIdByte => {
					self.id_byte = byte[0];
					self.transfer_state = TransferState::FindOverheadByte;	
				},
				TransferState::FindOverheadByte => {
					self.overhead_byte = byte[0];
					self.transfer_state = TransferState::FindPayloadLength;
				},
				TransferState::FindPayloadLength => {
					if byte[0] > 0 && byte[0] < MAX_PACKET_SIZE {
						self.payload_length = byte[0];
						self.transfer_state = TransferState::FindPayload;
						self.payload.clear();
					}else{
						self.transfer_state = TransferState::FindStartByte;
						self.status = TransferStatus::PayloadError;
					}
				},
				TransferState::FindPayload => {
					if self.payload.len() < self.payload_length.into() {
						self.payload.push(byte[0]);
	
						if self.payload.len() == self.payload_length.into() {
							self.transfer_state = TransferState::FindCrc;
						} else {
							self.transfer_state = TransferState::FindPayload;
						}
					}
				},
				TransferState::FindCrc => {
					let calculated_crc = self.crc.calculate(&self.payload,Some(self.payload_length));
					let received_crc = byte[0];

					if calculated_crc == received_crc {
						self.transfer_state = TransferState::FindStopByte;
					} else {
						self.transfer_state = TransferState::FindStartByte;
						self.status = TransferStatus::CrcError;
					}
				},
				TransferState::FindStopByte => {
					self.transfer_state = TransferState::FindStartByte;
	
					if byte[0] == STOP_BYTE {
						self.transfer_state = TransferState::FindStartByte;
						self.status = TransferStatus::NewData;
						let buffer : [u8;COUNT] = self.payload.clone().try_into().expect("Failed to convert payload to array");
						let dst : T = unsafe { transmute_copy(&buffer) };
						return Ok(Some(dst))
					} else {
						self.status = TransferStatus::StopByteError;
					}
				},
			}
	
			self.status = TransferStatus::Continue
		}

		Ok(None)
	}
}