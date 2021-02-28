# UDP data sender/receiver
Reads, sends and transmits records consiting of 32-bit id and a UTF-8 string through UDP.
Maximum record size is 508 bytes to ensure UDP packets are allowed anywhere.
To run:
```bash
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
cargo run
```
On NixOS `shell.nix` should provide all dependencies needed.
