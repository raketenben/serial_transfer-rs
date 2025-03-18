pub struct CRC {
    table: [u8; 256],
}

impl CRC {
    pub fn new(polynomial: u8) -> CRC {
        let mut table: [u8; 256] = [0; 256];
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
        CRC { table }
    }

    pub fn calculate(&self, data: &[u8]) -> u8 {
        let mut crc: u8 = 0;
        for i in 0..data.len() {
            let byte = match data.get(i as usize) {
                Some(byte) => byte,
                None => break,
            };
            crc = self.table[(crc ^ byte) as usize];
        }
        crc
    }
}
