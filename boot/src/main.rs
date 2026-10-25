#![no_std]
#![no_main]

use rost_elf::parse_header;
use uefi::boot;
use uefi::cstr16;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileMode};

// Limit how much of the kernel we read before allocating memory for it.
const MAX_HEADER_BYTES: usize = 16 * 1024;

#[entry]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }

    uefi::println!("rOSt bootloader v0.0.1");
    uefi::println!("UEFI initialized successfully.");

    match validate_kernel_header() {
        Ok(()) => {
            uefi::println!("Kernel ELF64 header validated.");
            Status::SUCCESS
        }
        Err(status) => {
            uefi::println!("Kernel validation failed: {:?}", status);
            status
        }
    }
}

// Locate the kernel and verify its ELF64 header.
fn validate_kernel_header() -> Result<(), Status> {
    let mut filesystem =
        boot::get_image_file_system(boot::image_handle()).map_err(|error| error.status())?;

    let mut root = filesystem.open_volume().map_err(|error| error.status())?;

    let file = root
        .open(
            cstr16!("kernel.elf"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .map_err(|error| error.status())?;

    let mut kernel = file.into_regular_file().ok_or(Status::LOAD_ERROR)?;

    let mut buffer = [0u8; MAX_HEADER_BYTES];
    let mut bytes_read = 0;

    // UEFI may return fewer bytes than requested, so continue until
    // the buffer is full or the end of the file is reached.
    while bytes_read < buffer.len() {
        let count = kernel
            .read(&mut buffer[bytes_read..])
            .map_err(|error| error.status())?;

        if count == 0 {
            break;
        }

        bytes_read += count;
    }

    let header = parse_header(&buffer[..bytes_read]).map_err(|error| {
        uefi::println!("Invalid kernel ELF: {:?}", error);
        Status::LOAD_ERROR
    })?;

    uefi::println!("Entry point: {:#x}", header.entry);
    uefi::println!("Program headers: {}", header.program_header_count);

    Ok(())
}
