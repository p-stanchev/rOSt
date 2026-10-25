#![no_std]

mod header;
mod plan;
mod segment;

pub use header::{ElfHeader, parse_header};
pub use plan::{ImageType, LoadSegment, MemoryPlan, PAGE_SIZE, plan_memory, visit_load_segments};
pub use segment::{LoadSummary, validate_load_segments};

#[derive(Debug, PartialEq, Eq)]
pub enum ElfError {
    InvalidMagic,
    UnsupportedFormat,
    MalformedHeader,
    MalformedSegment,
    Truncated,
    NoLoadableSegments,
    InvalidEntryPoint,
}

// Read a fixed number of bytes without going past the input.
fn read_bytes<const N: usize>(data: &[u8], offset: usize) -> Option<[u8; N]> {
    let end = offset.checked_add(N)?;
    data.get(offset..end)?.try_into().ok()
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(read_bytes(data, offset)?))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(read_bytes(data, offset)?))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(read_bytes(data, offset)?))
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::header::{
        ELF_CLASS_64, ELF_DATA_LITTLE_ENDIAN, ELF_MAGIC, ELF_VERSION, EM_X86_64, ET_EXEC,
        HEADER_SIZE, PROGRAM_HEADER_SIZE, field as header_field,
    };
    use crate::segment::{PF_EXECUTE, PF_READ, PT_LOAD, field as segment_field};

    pub const TEST_ENTRY: u64 = 0x1000;
    pub const TEST_FILE_SIZE: usize = HEADER_SIZE + PROGRAM_HEADER_SIZE;

    pub fn write_u16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn write_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    pub fn write_u64(data: &mut [u8], offset: usize, value: u64) {
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    // Build a small valid ELF image for our tests.
    pub fn sample() -> [u8; TEST_FILE_SIZE] {
        let mut data = [0u8; TEST_FILE_SIZE];

        data[..ELF_MAGIC.len()].copy_from_slice(&ELF_MAGIC);
        data[header_field::CLASS] = ELF_CLASS_64;
        data[header_field::DATA] = ELF_DATA_LITTLE_ENDIAN;
        data[header_field::IDENT_VERSION] = ELF_VERSION as u8;

        write_u16(&mut data, header_field::FILE_TYPE, ET_EXEC);
        write_u16(&mut data, header_field::MACHINE, EM_X86_64);
        write_u32(&mut data, header_field::VERSION, ELF_VERSION);
        write_u64(&mut data, header_field::ENTRY, TEST_ENTRY);
        write_u64(&mut data, header_field::PROGRAM_OFFSET, HEADER_SIZE as u64);

        write_u16(&mut data, header_field::HEADER_SIZE, HEADER_SIZE as u16);
        write_u16(
            &mut data,
            header_field::PROGRAM_ENTRY_SIZE,
            PROGRAM_HEADER_SIZE as u16,
        );
        write_u16(&mut data, header_field::PROGRAM_COUNT, 1);

        // Add one executable segment containing the entry point.
        let program = HEADER_SIZE;

        write_u32(&mut data, program + segment_field::TYPE, PT_LOAD);
        write_u32(
            &mut data,
            program + segment_field::FLAGS,
            PF_READ | PF_EXECUTE,
        );
        write_u64(&mut data, program + segment_field::FILE_OFFSET, 0);
        write_u64(
            &mut data,
            program + segment_field::VIRTUAL_ADDRESS,
            TEST_ENTRY,
        );
        write_u64(
            &mut data,
            program + segment_field::FILE_SIZE,
            TEST_FILE_SIZE as u64,
        );
        write_u64(
            &mut data,
            program + segment_field::MEMORY_SIZE,
            TEST_FILE_SIZE as u64,
        );
        write_u64(&mut data, program + segment_field::ALIGNMENT, 1);

        data
    }
}
