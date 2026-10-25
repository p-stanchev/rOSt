use crate::header::{PROGRAM_HEADER_SIZE, parse_header};
use crate::{ElfError, read_u32, read_u64};

pub(crate) const PT_LOAD: u32 = 1;
pub(crate) const PF_EXECUTE: u32 = 1;
#[cfg(test)]
pub(crate) const PF_READ: u32 = 4;

// Byte positions of the fields in an ELF64 program header.
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

/// Check loadable segments against the actual executable file size.
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

        // ELF segments must have compatible file and memory alignment.
        if alignment > 1 {
            if !alignment.is_power_of_two()
                || virtual_address % alignment != file_offset % alignment
            {
                return Err(MalformedSegment);
            }
        }

        if flags & PF_EXECUTE != 0 && header.entry >= virtual_address && header.entry < memory_end {
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
    use crate::header::HEADER_SIZE;
    use crate::test_support::{TEST_FILE_SIZE, sample, write_u32, write_u64};

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
    fn rejects_missing_loadable_segments() {
        let mut elf = sample();

        write_u32(&mut elf, HEADER_SIZE + field::TYPE, 0);

        assert_eq!(
            validate_load_segments(&elf, elf.len() as u64),
            Err(ElfError::NoLoadableSegments)
        );
    }
}
