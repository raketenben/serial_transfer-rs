# Serial Transfer Rust

Basic rust library to communicate with the [SerialTransfer](https://github.com/PowerBroker2/SerialTransfer) Arduino library

**This library does not implement the full functionality of the base library and was only tested for personal use cases**

## Usage

Initializing
```rust
//use serialport library to open the port
let port = serialport::new("COM4",9600).open().expect("Failed to open port");
//create serialtransfer
let mut transfer = SerialTransfer::new(port);
```

Some struct to transfer
```rust
struct DemoPackage {
	foo: u8,
	bar: u16
}
```
Sending
```rust
let demo_package = DemoPackage {foo:1,bar:42};
transfer.send::<DemoPackage,3>(packet_data).expect("Failed to send data");
```

Recieving
```rust
let result = transfer.available::<DemoPackage,3>().expect("Failed to read data");
match result {
	Some(data) => {
		println!("Foo: {} / Bar: {}",data.foo, data.bar);
	},
	None => {
		println!("No data");
	}
}
```

## Troubleshooting
In some cases you may get an IO-Timeout when sending packages, try to configure a timeout on your transfer to prevent this from happening