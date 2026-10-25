use crate::header::{PROGRAM_HEADER_SIZE, parse_header};
use crate::{ElfError, read_u32, read_u64};

pub(crate) const PT_LOAD: u32 = 1;
pub(crate) const PF_EXECUTE: u32 = 1;

#[cfg(test)]
pub(crate) const PF_READ: u32 = 4;

// Standard four-level x86-64 paging uses 48-bit virtual addresses.
const VIRTUAL_ADDRESS_BITS: u32 = 48;
const LOW_CANONICAL_END: u64 = 1u64 << (VIRTUAL_ADDRESS_BITS - 1);
const HIGH_CANONICAL_START: u64 = !(LOW_CANONICAL_END - 1);

// Byte positions of fields in an ELF64 program header.
pub(crate) mod field {
    pub const TYPE: usize = 0;
    pub const FLAGS: usize = 4;
    pub const FILE_OFFSET: usize = 8;
    pub const VIRTUAL_ADDRESS: usize = 16;
    pub const FILE_SIZE: usize = 32;
    pub const MEMORY_SIZE: usize = 40;
    pub const ALIGNMENT: usize = 48;
}

#[derive(Debug, PartialEq, Eq)]
pub struct LoadSummary {
    pub loadable_segments: u16,
}

// Check that an entire memory range uses canonical x86-64 addresses.
fn is_canonical_range(start: u64, end: u64) -> bool {
    let last = end - 1;

    (start < LOW_CANONICAL_END && last < LOW_CANONICAL_END)
        || (start >= HIGH_CANONICAL_START && last >= HIGH_CANONICAL_START)
}

/// Check the loadable segments and executable entry point.
pub fn validate_load_segments(data: &[u8], file_size: u64) -> Result<LoadSummary, ElfError> {
    use ElfError::*;

    let header = parse_header(data)?;
    let mut loadable_segments = 0u16;
    let mut entry_is_executable = false;

    for index in 0..usize::from(header.program_header_count) {
        let offset = header.program_header_offset + index * PROGRAM_HEADER_SIZE;

        let segment_type = read_u32(data, offset + field::TYPE).ok_or(Truncated)?;

        if segment_type != PT_LOAD {
            continue;
        }

        let flags = read_u32(data, offset + field::FLAGS).ok_or(Truncated)?;
        let file_offset = read_u64(data, offset + field::FILE_OFFSET).ok_or(Truncated)?;
        let virtual_address = read_u64(data, offset + field::VIRTUAL_ADDRESS).ok_or(Truncated)?;
        let file_bytes = read_u64(data, offset + field::FILE_SIZE).ok_or(Truncated)?;
        let memory_bytes = read_u64(data, offset + field::MEMORY_SIZE).ok_or(Truncated)?;
        let alignment = read_u64(data, offset + field::ALIGNMENT).ok_or(Truncated)?;

        if memory_bytes == 0 || file_bytes > memory_bytes {
            return Err(MalformedSegment);
        }

        let file_end = file_offset
            .checked_add(file_bytes)
            .ok_or(MalformedSegment)?;

        if file_end > file_size {
            return Err(Truncated);
        }

        let memory_end = virtual_address
            .checked_add(memory_bytes)
            .ok_or(MalformedSegment)?;

        if !is_canonical_range(virtual_address, memory_end) {
            return Err(MalformedSegment);
        }

        if alignment > 1
            && (!alignment.is_power_of_two()
                || virtual_address % alignment != file_offset % alignment)
        {
            return Err(MalformedSegment);
        }

        // Earlier loadable segments must not occupy the same memory.
        for previous_index in 0..index {
            let previous_offset =
                header.program_header_offset + previous_index * PROGRAM_HEADER_SIZE;

            let previous_type = read_u32(data, previous_offset + field::TYPE).ok_or(Truncated)?;

            if previous_type != PT_LOAD {
                continue;
            }

            let previous_start =
                read_u64(data, previous_offset + field::VIRTUAL_ADDRESS).ok_or(Truncated)?;

            let previous_size =
                read_u64(data, previous_offset + field::MEMORY_SIZE).ok_or(Truncated)?;

            let previous_end = previous_start
                .checked_add(previous_size)
                .ok_or(MalformedSegment)?;

            if virtual_address < previous_end && previous_start < memory_end {
                return Err(MalformedSegment);
            }
        }

        // The entry point must refer to bytes actually present in the file.
        let entry_offset = header.entry.checked_sub(virtual_address);

        if flags & PF_EXECUTE != 0 && entry_offset.is_some_and(|offset| offset < file_bytes) {
            entry_is_executable = true;
        }

        loadable_segments += 1;
    }

    if loadable_segments == 0 {
        return Err(NoLoadableSegments);
    }

    if !entry_is_executable {
        return Err(InvalidEntryPoint);
    }

    Ok(LoadSummary { loadable_segments })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{HEADER_SIZE, field as header_field};
    use crate::test_support::{
        TEST_ENTRY, TEST_FILE_SIZE, sample, write_u16, write_u32, write_u64,
    };

    // Build two separate loadable segments to test their memory layout.
    fn two_segment_image() -> [u8; HEADER_SIZE + PROGRAM_HEADER_SIZE * 2 + 16] {
        let mut elf = [0u8; HEADER_SIZE + PROGRAM_HEADER_SIZE * 2 + 16];

        elf[..TEST_FILE_SIZE].copy_from_slice(&sample());
        write_u16(&mut elf, header_field::PROGRAM_COUNT, 2);

        let second = HEADER_SIZE + PROGRAM_HEADER_SIZE;
        let second_file_offset = HEADER_SIZE + PROGRAM_HEADER_SIZE * 2;

        write_u32(&mut elf, second + field::TYPE, PT_LOAD);
        write_u32(&mut elf, second + field::FLAGS, PF_READ);
        write_u64(
            &mut elf,
            second + field::FILE_OFFSET,
            second_file_offset as u64,
        );
        write_u64(
            &mut elf,
            second + field::VIRTUAL_ADDRESS,
            TEST_ENTRY + TEST_FILE_SIZE as u64,
        );
        write_u64(&mut elf, second + field::FILE_SIZE, 16);
        write_u64(&mut elf, second + field::MEMORY_SIZE, 32);
        write_u64(&mut elf, second + field::ALIGNMENT, 1);

        elf
    }

    #[test]
    fn accepts_loadable_segment() {
        let elf = sample();

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Ok(LoadSummary {
                loadable_segments: 1
            })
        );
    }

    #[test]
    fn accepts_adjacent_segments() {
        let elf = two_segment_image();

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Ok(LoadSummary {
                loadable_segments: 2
            })
        );
    }

    #[test]
    fn accepts_high_canonical_addresses() {
        let mut elf = sample();

        write_u64(
            &mut elf,
            HEADER_SIZE + field::VIRTUAL_ADDRESS,
            HIGH_CANONICAL_START,
        );
        write_u64(&mut elf, header_field::ENTRY, HIGH_CANONICAL_START);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Ok(LoadSummary {
                loadable_segments: 1
            })
        );
    }

    #[test]
    fn rejects_segment_outside_file() {
        let elf = sample();

        assert_eq!(
            validate_load_segments(&elf, (elf.len() - 1) as u64),
            Err(ElfError::Truncated)
        );
    }

    #[test]
    fn rejects_segment_larger_on_disk_than_memory() {
        let mut elf = sample();

        write_u64(
            &mut elf,
            HEADER_SIZE + field::MEMORY_SIZE,
            (TEST_FILE_SIZE - 1) as u64,
        );

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_non_executable_entry() {
        let mut elf = sample();

        write_u32(&mut elf, HEADER_SIZE + field::FLAGS, PF_READ);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::InvalidEntryPoint)
        );
    }

    #[test]
    fn rejects_entry_in_zero_filled_memory() {
        let mut elf = sample();

        write_u64(&mut elf, HEADER_SIZE + field::FILE_SIZE, 16);
        write_u64(&mut elf, header_field::ENTRY, TEST_ENTRY + 16);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::InvalidEntryPoint)
        );
    }

    #[test]
    fn rejects_overlapping_segments() {
        let mut elf = two_segment_image();
        let second = HEADER_SIZE + PROGRAM_HEADER_SIZE;

        write_u64(&mut elf, second + field::VIRTUAL_ADDRESS, TEST_ENTRY + 64);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_invalid_alignment() {
        let mut elf = sample();

        write_u64(&mut elf, HEADER_SIZE + field::ALIGNMENT, 3);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_virtual_address_overflow() {
        let mut elf = sample();

        write_u64(&mut elf, HEADER_SIZE + field::VIRTUAL_ADDRESS, u64::MAX - 8);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_noncanonical_address() {
        let mut elf = sample();

        write_u64(
            &mut elf,
            HEADER_SIZE + field::VIRTUAL_ADDRESS,
            LOW_CANONICAL_END,
        );

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_range_crossing_canonical_boundary() {
        let mut elf = sample();

        write_u64(
            &mut elf,
            HEADER_SIZE + field::VIRTUAL_ADDRESS,
            LOW_CANONICAL_END - 8,
        );

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_missing_loadable_segments() {
        let mut elf = sample();

        write_u32(&mut elf, HEADER_SIZE + field::TYPE, 0);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::NoLoadableSegments)
        );
    }
}
