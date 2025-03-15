#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::io::Write;

    use crate::SerialTransfer;
    use crate::START_BYTE;

    /// MockPipe just mirrors the data written to it
    /// Simulating a peer which just echoes the data back
    struct MockPipe {
        data: Vec<u8>,
    }

    impl MockPipe {
        fn new() -> Self {
            MockPipe { data: Vec::new() }
        }
    }

    impl Write for MockPipe {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Read for MockPipe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let len = std::cmp::min(buf.len(), self.data.len());
            buf[..len].copy_from_slice(&self.data[..len]);
            self.data = self.data.split_off(len);
            Ok(len)
        }
    }

    #[test]
    fn compile_test_serialport() {
        // We don't care about the result, just that it compiles
        let port = serialport::new("", 9600).open();
        if let Ok(port) = port {
            let _ = SerialTransfer::from(port);
        }
        assert!(true);
    }

    #[test]
    fn basic() {
        let mut mock_port = MockPipe::new();
        let mut transfer = SerialTransfer::new(&mut mock_port);

        let test_data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

        transfer
            .send::<[u8; 8], 8>(test_data)
            .expect("Error sending data");

        let data = transfer
            .available::<[u8; 8], 8>()
            .expect("Error reading data");
        if let Some(data) = data {
            println!("Got data: {:?}", data);
            assert!(data == test_data);
        }
    }

    #[test]
    fn cobs() {
        let mut mock_port = MockPipe::new();
        let mut transfer = SerialTransfer::new(&mut mock_port);

        let test_data = [
            0x00, START_BYTE, 0x01, START_BYTE, 0x02, 0x03, START_BYTE, START_BYTE,
        ];

        transfer
            .send::<[u8; 8], 8>(test_data)
            .expect("Error sending data");

        let data = transfer
            .available::<[u8; 8], 8>()
            .expect("Error reading data");
        if let Some(data) = data {
            println!("Got data: {:?}", data);
            assert!(data == test_data);
        }
    }

    #[test]
    fn end_to_end_true() {
        let mut mock_port = MockPipe::new();
        let mut transfer = SerialTransfer::new(&mut mock_port);

        let test_data = [126, 126, 126, 126, 126];

        transfer
            .send::<[u8; 5], 5>(test_data)
            .expect("Error sending data");

        let data = transfer
            .available::<[u8; 5], 5>()
            .expect("Error reading data");
        if let Some(data) = data {
            println!("Got data: {:?}", data);
            assert!(data == test_data);
        }
    }

    #[test]
    fn end_to_end_false() {
        let mut mock_port = MockPipe::new();
        let mut transfer = SerialTransfer::new(&mut mock_port);

        //let test_data = [126, 126, 126, 126, 126];
        let neg_test_data = [1, 1, 1, 1, 1];

        transfer
            .send::<[u8; 5], 5>(neg_test_data)
            .expect("Error sending data");

        let data = transfer
            .available::<[u8; 5], 5>()
            .expect("Error reading data");
        if let Some(data) = data {
            println!("Got data: {:?}", data);
            assert!(data == neg_test_data);
        }
    }
}
