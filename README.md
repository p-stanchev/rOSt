# rOSt

An experimental modular monolithic operating system written in Rust, focused on security, efficiency, and simplicity.

## Status

rOSt is in early development. The project currently consists of a custom UEFI bootloader and a minimal freestanding x86-64 kernel, both written in Rust.

The bootloader initializes UEFI services, displays a startup message, and locates the kernel executable by checking its ELF signature. This functionality has been tested using QEMU with EDK2 firmware.

The kernel does not execute yet.

## Building

### Requirements

- Rust and Cargo
- QEMU with EDK2/OVMF firmware
- `x86_64-unknown-uefi` and `x86_64-unknown-none` Rust targets

Install the required targets:

```bash
rustup target add x86_64-unknown-uefi x86_64-unknown-none
```

Build the bootloader and kernel:

```bash
cargo build -p rost-boot --target x86_64-unknown-uefi
cargo build -p rost-kernel --target x86_64-unknown-none
```

The resulting executables are located at:

```text
target/x86_64-unknown-uefi/debug/rost-boot.efi
target/x86_64-unknown-none/debug/rost-kernel
```

## Running

Copy the compiled EFI executable to `EFI/BOOT/BOOTX64.EFI` on a FAT volume.

Copy the compiled kernel to `kernel.elf` at the root of the same volume.

Launch QEMU with x86-64 EDK2 firmware and the volume attached as a virtual disk.

The bootloader checks the kernel's ELF signature but does not load or execute its program segments.

## License

rOSt is licensed under the [MIT License](LICENSE).