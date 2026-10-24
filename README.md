
# rOSt

An experimental modular monolithic operating system written in Rust, focused on security, efficiency, and simplicity.

## Status

rOSt is in early development. The project currently consists of a custom UEFI bootloader, a minimal freestanding x86-64 kernel, and an ELF64 validation library, all written in Rust.

The bootloader initializes UEFI services, displays a startup message, and locates the kernel executable by checking its ELF signature. This functionality has been tested using QEMU with EDK2 firmware.

The ELF library validates basic ELF64 headers, architecture, executable format, and program-header table boundaries. Its unit tests pass, but the library is not yet integrated into the bootloader.

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

Build the bootloader, kernel, and ELF library:

```bash
cargo build -p rost-boot --target x86_64-unknown-uefi
cargo build -p rost-kernel --target x86_64-unknown-none
cargo build -p rost-elf --target x86_64-unknown-uefi
```

The resulting bootloader and kernel executables are located at:

```text
target/x86_64-unknown-uefi/debug/rost-boot.efi
target/x86_64-unknown-none/debug/rost-kernel
```

## Testing

Run the ELF library's unit tests:

```bash
cargo test -p rost-elf
```

The bootloader has also been tested using QEMU with EDK2/OVMF firmware.

## Running

Copy the compiled EFI executable to `EFI/BOOT/BOOTX64.EFI` on a FAT volume.

Copy the compiled kernel to `kernel.elf` at the root of the same volume.

Launch QEMU with x86-64 EDK2 firmware and the volume attached as a virtual disk.

The bootloader currently checks the kernel's ELF signature but does not load or execute its program segments.

## License

rOSt is licensed under the [MIT License](LICENSE).