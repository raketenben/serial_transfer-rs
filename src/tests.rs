#[cfg(test)]
mod tests {

    use embedded_io::ErrorType;
    #[cfg(not(feature = "std"))]
    use embedded_io::{Read, Write};
    #[cfg(feature = "std")]
    use std::io::{Read, Write};

    use crate::SerialTransfer;
    use crate::TransferError;
    use crate::START_BYTE;
    use crate::STOP_BYTE;

    /// MockPipe just mirrors the data written to it
    /// Simulating a peer which just echoes the data back

    struct MockPipe {
        data: Vec<u8>,
        /// If set, will change the byte at this index to be invalid
        invalidate_byte_at: Option<u8>,
        return_random: bool,
    }

    impl MockPipe {
        fn new() -> Self {
            MockPipe {
                data: Vec::new(),
                invalidate_byte_at: None,
                return_random: false,
            }
        }

        fn with_invalidate_byte(invalidate_byte_at: u8) -> Self {
            MockPipe {
                data: Vec::new(),
                invalidate_byte_at: Some(invalidate_byte_at),
                return_random: false,
            }
        }

        #[cfg(feature = "std")]
        fn with_random() -> Self {
            MockPipe {
                data: Vec::new(),
                invalidate_byte_at: None,
                return_random: true,
            }
        }
    }

    impl ErrorType for MockPipe {
        type Error = TransferError;
    }

    impl Write for MockPipe {
        fn write(&mut self, buf: &[u8]) -> Result<usize, TransferError> {
            //append
            self.data.extend_from_slice(buf);

            if let Some(invalidate_byte_at) = self.invalidate_byte_at {
                self.data[invalidate_byte_at as usize] = self.data[invalidate_byte_at as usize] ^ 1;
            }

            Ok(buf.len())
        }
        fn flush(&mut self) -> Result<(), TransferError> {
            Ok(())
        }
    }

    impl Read for MockPipe {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransferError> {
            let len = buf.len().min(self.data.len());
            buf[..len].copy_from_slice(&self.data[..len]);
            self.data = self.data.split_off(len);

            if self.return_random {
                for i in 0..buf.len() {
                    buf[i] = rand::random();
                }
            }

            Ok(len)
        }
    }

    #[cfg(all(feature = "serialport", feature = "std"))]
    #[test]
    fn compile_test_serialport() {
        // We don't care about the result, just that it compiles

        use serialport::SerialPort;
        let port = serialport::new("", 9600).open();
        if let Ok(port) = port {
            let _: SerialTransfer<Box<dyn SerialPort>, u8, 1> = SerialTransfer::from(port);
        }
        assert!(true);
    }

    fn normal_transfer<T: PartialEq + Copy, const COUNT: usize>(test_data: T)
    where
        [(); COUNT + 3]:,
        [(); COUNT + 6]:,
    {
        let mut mock_port = MockPipe::new();
        let mut transfer = SerialTransfer::new(&mut mock_port);

        transfer
            .send(test_data.clone())
            .expect("Error sending data");

        let data = transfer.available().expect("Error reading data");

        assert!(data.is_some());
        assert!(data.unwrap() == test_data);
    }

    #[test]
    fn basic() {
        normal_transfer::<[u8; 8], 8>([1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn cobs() {
        normal_transfer::<[u8; 8], 8>([START_BYTE, START_BYTE, 3, 4, START_BYTE, 6, START_BYTE, 8]);
    }

    #[test]
    fn premature_end() {
        normal_transfer::<[u8; 8], 8>([STOP_BYTE, START_BYTE, 3, 4, START_BYTE, 3, 4, STOP_BYTE]);
    }

    fn corrupted_transfer<T, const COUNT: usize>(test_data: T, corrupted_byte: u8)
    where
        [(); COUNT + 3]:,
        [(); COUNT + 6]:,
    {
        let mut mock_port = MockPipe::with_invalidate_byte(corrupted_byte);
        let mut transfer = SerialTransfer::new(&mut mock_port);

        transfer.send(test_data).expect("Error sending data");

        let data = transfer.available().expect("Error reading data");

        assert!(data.is_none());
    }

    //tests for error detection in the CRC
    #[test]
    fn crc_start_byte() {
        corrupted_transfer::<[u8; 5], 5>([1, STOP_BYTE, 3, 4, START_BYTE], 0);
    }

    #[test]
    fn crc_id_byte() {
        corrupted_transfer::<[u8; 5], 5>([1, STOP_BYTE, 3, 4, START_BYTE], 1);
    }

    #[test]
    fn crc_overhead_byte() {
        corrupted_transfer::<[u8; 5], 5>([1, STOP_BYTE, 3, 4, START_BYTE], 2);
    }

    #[test]
    fn crc_payload_length() {
        corrupted_transfer::<[u8; 5], 5>([1, STOP_BYTE, 3, 4, START_BYTE], 3);
    }

    #[test]
    fn crc_payload() {
        corrupted_transfer::<[u8; 5], 5>([1, STOP_BYTE, 3, 4, START_BYTE], 4);
    }

    //recovery tests
    #[test]
    fn recovery() {
        for i in 0..11 {
            corrupted_transfer::<[u8; 5], 5>([1, 2, 3, 4, 5], i);
            normal_transfer::<[u8; 5], 5>([1, 2, 3, 4, 5]);
        }
    }

    #[test]
    fn recovery_cobs() {
        for i in 0..11 {
            corrupted_transfer::<[u8; 5], 5>([START_BYTE, 2, 3, 4, 5], i);
            normal_transfer::<[u8; 5], 5>([START_BYTE, 2, 3, 4, 5]);
        }
    }

    #[test]
    fn recovery_premature_end() {
        for i in 0..11 {
            corrupted_transfer::<[u8; 5], 5>([STOP_BYTE, 2, 3, 4, 5], i);
            normal_transfer::<[u8; 5], 5>([STOP_BYTE, 2, 3, 4, 5]);
        }
    }

    //random noise tests
    //make sure there are no false positives
    #[test]
    #[cfg(feature = "std")]
    fn random_noise() {
        let mut mock_port = MockPipe::with_random();
        let mut transfer: SerialTransfer<&mut _, [u8; 5], 5> = SerialTransfer::new(&mut mock_port);

        for _ in 0..100000 {
            transfer.send([1, 2, 3, 4, 5]).expect("Error sending data");

            let data = transfer.available().expect("Error reading data");

            assert!(data.is_none());
        }
    }
}
