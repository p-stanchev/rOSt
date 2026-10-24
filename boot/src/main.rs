
#![no_std]
#![no_main]

use uefi::prelude::*;

#[entry]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }

    uefi::println!("rOSt bootloader v0.0.1");
    uefi::println!("UEFI initialized successfully.");

    Status::SUCCESS
}
