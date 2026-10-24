#![no_std]
#![no_main]

use rost_elf::parse_header;
use uefi::boot;
use uefi::cstr16;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileMode};

const HEADER_BUFFER_SIZE: usize = 16 * 1024;

#[entry]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }

    uefi::println!("rOSt bootloader v0.0.1");
    uefi::println!("UEFI initialized successfully.");

    match check_kernel() {
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

fn check_kernel() -> Result<(), Status> {
    let mut fs = boot::get_image_file_system(boot::image_handle()).map_err(|err| err.status())?;

    let mut root = fs.open_volume().map_err(|err| err.status())?;

    let file = root
        .open(
            cstr16!("kernel.elf"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .map_err(|err| err.status())?;

    let mut kernel = file.into_regular_file().ok_or(Status::LOAD_ERROR)?;

    let mut buffer = [0u8; HEADER_BUFFER_SIZE];
    let mut bytes_read = 0;

    while bytes_read < buffer.len() {
        let count = kernel
            .read(&mut buffer[bytes_read..])
            .map_err(|err| err.status())?;

        if count == 0 {
            break;
        }

        bytes_read += count;
    }

    let header = parse_header(&buffer[..bytes_read]).map_err(|err| {
        uefi::println!("Invalid kernel ELF: {:?}", err);
        Status::LOAD_ERROR
    })?;

    uefi::println!("Entry point: {:#x}", header.entry);
    uefi::println!("Program headers: {}", header.program_header_count);

    Ok(())
}
