
#![no_std]
#![no_main]

use uefi::boot;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::cstr16;

#[entry]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }

    uefi::println!("rOSt bootloader v0.0.1");
    uefi::println!("UEFI initialized successfully.");

    match check_kernel() {
        Ok(()) => {
            uefi::println!("Kernel ELF signature verified.");
            Status::SUCCESS
        }
        Err(status) => {
            uefi::println!("Kernel check failed: {:?}", status);
            status
        }
    }
}

fn check_kernel() -> Result<(), Status> {
    let mut fs = boot::get_image_file_system(boot::image_handle())
        .map_err(|err| err.status())?;

    let mut root = fs.open_volume()
        .map_err(|err| err.status())?;

    let file = root.open(
        cstr16!("kernel.elf"),
        FileMode::Read,
        FileAttribute::empty(),
    ).map_err(|err| err.status())?;

    let mut kernel = file.into_regular_file()
        .ok_or(Status::LOAD_ERROR)?;

    let mut signature = [0u8; 4];
    let bytes_read = kernel.read(&mut signature)
        .map_err(|err| err.status())?;

    if bytes_read != signature.len() || signature != *b"\x7FELF" {
        return Err(Status::LOAD_ERROR);
    }

    Ok(())
}
