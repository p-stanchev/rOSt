# rOSt

An experimental modular monolithic operating system written in Rust, focused on security, efficiency, and simplicity.

## Status

rOSt is in early development. The project currently consists of a custom UEFI bootloader written in Rust.

The bootloader initializes UEFI services and displays a startup message. It has been tested using QEMU with EDK2 firmware.

The kernel itself is not yet implemented.

## Building

### Requirements

- Rust and Cargo
- QEMU with EDK2/OVMF firmware
- `x86_64-unknown-uefi` Rust target

Install the required target:

```bash
rustup target add x86_64-unknown-uefi
```

Build the bootloader:

```bash
cargo build -p rost-boot --target x86_64-unknown-uefi
```

The resulting executable is located at:

```text
target/x86_64-unknown-uefi/debug/rost-boot.efi
```

## Running

Copy the compiled EFI executable to `EFI/BOOT/BOOTX64.EFI` on a FAT volume.

Launch QEMU with x86-64 EDK2 firmware and the volume attached as a virtual disk.

## License

rOSt is licensed under the [MIT License](LICENSE).