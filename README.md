# Visca IP Converter
Converts visca over ip to a local serial port. Used with a Blackmagic ATEM with a CV620-WH camera as a test enviorment.


MacOS
```
RUST_LOG=info cargo run -- --listen-address  0.0.0.0:52381 --serial-device /dev/tty.PL2303G-USBtoUART210
```
