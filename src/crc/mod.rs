

pub struct CRC {
	table : [u8;256],
}

impl CRC {
	pub fn new(polynomial : u8) -> CRC {
		let mut table : [u8;256] = [0;256];
		for i in 0..255 {
			let mut crc = i;
			for _ in 0..8 {
				if crc & 0x80 != 0 {
					crc = (crc << 1) ^ polynomial;
				} else {
					crc <<= 1;
				}
			}
			table[i as usize] = crc;
		}
		CRC {
			table,
		}
	}

	pub fn calculate(&self,data : &Vec<u8>, length : Option<u8>) -> u8 {
		let length = match length {
			Some(length) => length,
			None => data.len() as u8,
		};

		let mut crc : u8 = 0;
		for i in 0..length {
			let byte = match data.get(i as usize) {
				Some(byte) => byte,
				None => break,
			};
			crc = self.table[(crc ^ byte) as usize];
		}
		crc
	}
}