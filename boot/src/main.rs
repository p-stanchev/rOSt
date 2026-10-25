#![no_std]
#![no_main]

use rost_elf::{PAGE_SIZE, inspect_dynamic, parse_header, plan_memory, visit_load_segments};
use uefi::boot;
use uefi::cstr16;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileMode, RegularFile};

// Maximum amount of ELF header and metadata we inspect.
const MAX_HEADER_BYTES: usize = 16 * 1024;

#[entry]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }

    uefi::println!("rOSt bootloader v0.0.1");
    uefi::println!("UEFI initialized successfully.");

    match inspect_kernel() {
        Ok(()) => {
            uefi::println!("Kernel ELF64 layout inspected.");
            Status::SUCCESS
        }
        Err(status) => {
            uefi::println!("Kernel inspection failed: {:?}", status);
            status
        }
    }
}

// Inspect the kernel without loading it into memory.
fn inspect_kernel() -> Result<(), Status> {
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

    kernel
        .set_position(RegularFile::END_OF_FILE)
        .map_err(|error| error.status())?;

    let file_size = kernel.get_position().map_err(|error| error.status())?;

    kernel.set_position(0).map_err(|error| error.status())?;

    let mut buffer = [0u8; MAX_HEADER_BYTES];
    let mut bytes_read = 0;

    // UEFI may return fewer bytes than requested.
    while bytes_read < buffer.len() {
        let count = kernel
            .read(&mut buffer[bytes_read..])
            .map_err(|error| error.status())?;

        if count == 0 {
            break;
        }

        bytes_read += count;
    }

    let data = &buffer[..bytes_read];

    let header = parse_header(data).map_err(|error| {
        uefi::println!("Invalid kernel header: {:?}", error);
        Status::LOAD_ERROR
    })?;

    let plan = plan_memory(data, file_size).map_err(|error| {
        uefi::println!("Invalid kernel memory layout: {:?}", error);
        Status::LOAD_ERROR
    })?;

    // PT_DYNAMIC describes possible runtime relocation requirements.
    let dynamic = inspect_dynamic(data, file_size).map_err(|error| {
        uefi::println!("Invalid kernel dynamic metadata: {:?}", error);
        Status::LOAD_ERROR
    })?;

    uefi::println!("Entry point: {:#x}", header.entry);
    uefi::println!("Program headers: {}", header.program_header_count);
    uefi::println!("Loadable segments: {}", plan.loadable_segments);

    visit_load_segments(data, file_size, |segment| {
        uefi::println!(
            "VA {:#x}, file {:#x}",
            segment.virtual_address,
            segment.file_offset
        );
        uefi::println!(
            "  Size: {:#x} / {:#x}",
            segment.file_size,
            segment.memory_size
        );
    })
    .map_err(|error| {
        uefi::println!("Invalid kernel segments: {:?}", error);
        Status::LOAD_ERROR
    })?;

    uefi::println!("Image type: {:?}", plan.image_type);
    uefi::println!(
        "Page span: {:#x}..{:#x}",
        plan.virtual_start,
        plan.virtual_end
    );
    uefi::println!("Pages ({} bytes): {}", PAGE_SIZE, plan.page_count);

    if dynamic.present {
        uefi::println!("Dynamic metadata: present");
        uefi::println!(
            "RELA/REL/RELR bytes: {}/{}/{}",
            dynamic.rela_bytes,
            dynamic.rel_bytes,
            dynamic.relr_bytes
        );
        uefi::println!("PLT relocation bytes: {}", dynamic.plt_bytes);
        uefi::println!("Required libraries: {}", dynamic.needed_libraries);
        uefi::println!("Text relocations: {}", dynamic.text_relocations);
    } else {
        uefi::println!("Dynamic metadata: absent");
    }

    uefi::println!(
        "Dynamic relocation tables declared: {}",
        dynamic.has_relocation_tables()
    );

    Ok(())
}
