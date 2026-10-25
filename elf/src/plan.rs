use crate::header::{self, PROGRAM_HEADER_SIZE, parse_header};
use crate::segment::{self, PT_LOAD, validate_load_segments};
use crate::{ElfError, read_u16, read_u32, read_u64};

// The current x86-64 loader will use 4 KiB pages.
pub const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Executable,
    PositionIndependent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSegment {
    pub file_offset: u64,
    pub virtual_address: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub alignment: u64,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryPlan {
    pub image_type: ImageType,
    pub virtual_start: u64,
    pub virtual_end: u64,
    pub page_count: u64,
    pub loadable_segments: u16,
}

/// Visit each loadable segment after validating the complete image.
pub fn visit_load_segments(
    data: &[u8],
    file_size: u64,
    mut visit: impl FnMut(LoadSegment),
) -> Result<ImageType, ElfError> {
    use ElfError::*;

    // Validate all segments before exposing any of them.
    validate_load_segments(data, file_size)?;

    let header = parse_header(data)?;

    let image_type = match read_u16(data, header::field::FILE_TYPE) {
        Some(header::ET_EXEC) => ImageType::Executable,
        Some(header::ET_DYN) => ImageType::PositionIndependent,
        _ => return Err(UnsupportedFormat),
    };

    for index in 0..usize::from(header.program_header_count) {
        let offset = header.program_header_offset + index * PROGRAM_HEADER_SIZE;

        let segment_type = read_u32(data, offset + segment::field::TYPE).ok_or(Truncated)?;

        if segment_type != PT_LOAD {
            continue;
        }

        let segment = LoadSegment {
            file_offset: read_u64(data, offset + segment::field::FILE_OFFSET).ok_or(Truncated)?,
            virtual_address: read_u64(data, offset + segment::field::VIRTUAL_ADDRESS)
                .ok_or(Truncated)?,
            file_size: read_u64(data, offset + segment::field::FILE_SIZE).ok_or(Truncated)?,
            memory_size: read_u64(data, offset + segment::field::MEMORY_SIZE).ok_or(Truncated)?,
            alignment: read_u64(data, offset + segment::field::ALIGNMENT).ok_or(Truncated)?,
            flags: read_u32(data, offset + segment::field::FLAGS).ok_or(Truncated)?,
        };

        visit(segment);
    }

    Ok(image_type)
}

fn round_down_to_page(address: u64) -> u64 {
    address - address % PAGE_SIZE
}

fn round_up_to_page(address: u64) -> Option<u64> {
    let remainder = address % PAGE_SIZE;

    if remainder == 0 {
        Some(address)
    } else {
        address.checked_add(PAGE_SIZE - remainder)
    }
}

/// Calculate the page-aligned virtual span without allocating memory.
pub fn plan_memory(data: &[u8], file_size: u64) -> Result<MemoryPlan, ElfError> {
    let mut lowest_address = u64::MAX;
    let mut highest_end = 0u64;
    let mut loadable_segments = 0u16;

    let image_type = visit_load_segments(data, file_size, |segment| {
        lowest_address = lowest_address.min(segment.virtual_address);

        // Segment validation already checked this addition for overflow.
        let end = segment.virtual_address + segment.memory_size;
        highest_end = highest_end.max(end);

        loadable_segments += 1;
    })?;

    let virtual_start = round_down_to_page(lowest_address);
    let virtual_end = round_up_to_page(highest_end).ok_or(ElfError::MalformedSegment)?;

    let span = virtual_end
        .checked_sub(virtual_start)
        .ok_or(ElfError::MalformedSegment)?;

    Ok(MemoryPlan {
        image_type,
        virtual_start,
        virtual_end,
        page_count: span / PAGE_SIZE,
        loadable_segments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{HEADER_SIZE, field as header_field};
    use crate::segment::{PF_READ, field as segment_field};
    use crate::test_support::{TEST_ENTRY, sample, write_u16, write_u32, write_u64};

    #[test]
    fn reports_executable_segments() {
        let elf = sample();
        let mut found = None;

        let image_type =
            visit_load_segments(&elf, elf.len() as u64, |segment| found = Some(segment)).unwrap();

        assert_eq!(image_type, ImageType::Executable);

        let segment = found.unwrap();
        assert_eq!(segment.virtual_address, TEST_ENTRY);
        assert_eq!(segment.file_offset, 0);
        assert_eq!(segment.file_size, elf.len() as u64);
        assert_eq!(segment.memory_size, elf.len() as u64);
    }

    #[test]
    fn reports_position_independent_images() {
        let mut elf = sample();

        write_u16(&mut elf, header_field::FILE_TYPE, header::ET_DYN);

        let image_type = visit_load_segments(&elf, elf.len() as u64, |_| {}).unwrap();

        assert_eq!(image_type, ImageType::PositionIndependent);
    }

    #[test]
    fn rejects_invalid_image_before_visiting() {
        let mut elf = sample();

        write_u64(&mut elf, HEADER_SIZE + segment_field::FILE_SIZE, u64::MAX);

        let mut visits = 0;

        let result = visit_load_segments(&elf, elf.len() as u64, |_| visits += 1);

        assert_eq!(result, Err(ElfError::MalformedSegment));
        assert_eq!(visits, 0);
    }

    #[test]
    fn calculates_single_page() {
        let elf = sample();
        let plan = plan_memory(&elf, elf.len() as u64).unwrap();

        assert_eq!(plan.image_type, ImageType::Executable);
        assert_eq!(plan.virtual_start, 0x1000);
        assert_eq!(plan.virtual_end, 0x2000);
        assert_eq!(plan.page_count, 1);
        assert_eq!(plan.loadable_segments, 1);
    }

    #[test]
    fn includes_zero_initialized_memory() {
        let mut elf = sample();

        write_u64(
            &mut elf,
            HEADER_SIZE + segment_field::MEMORY_SIZE,
            PAGE_SIZE * 2 + 1,
        );

        let plan = plan_memory(&elf, elf.len() as u64).unwrap();

        assert_eq!(plan.virtual_start, 0x1000);
        assert_eq!(plan.virtual_end, 0x4000);
        assert_eq!(plan.page_count, 3);
    }

    #[test]
    fn rounds_down_unaligned_start() {
        let mut elf = sample();
        let start = 0x1F80;

        write_u64(
            &mut elf,
            HEADER_SIZE + segment_field::VIRTUAL_ADDRESS,
            start,
        );
        write_u64(&mut elf, header_field::ENTRY, start);

        let plan = plan_memory(&elf, elf.len() as u64).unwrap();

        assert_eq!(plan.virtual_start, 0x1000);
        assert_eq!(plan.virtual_end, 0x2000);
        assert_eq!(plan.page_count, 1);
    }

    #[test]
    fn does_not_add_an_extra_aligned_page() {
        let mut elf = sample();

        write_u64(
            &mut elf,
            HEADER_SIZE + segment_field::MEMORY_SIZE,
            PAGE_SIZE,
        );

        let plan = plan_memory(&elf, elf.len() as u64).unwrap();

        assert_eq!(plan.virtual_end, 0x2000);
        assert_eq!(plan.page_count, 1);
    }

    #[test]
    fn includes_gaps_between_segments() {
        const SECOND_HEADER: usize = HEADER_SIZE + PROGRAM_HEADER_SIZE;
        const SECOND_DATA: usize = HEADER_SIZE + PROGRAM_HEADER_SIZE * 2;
        const IMAGE_SIZE: usize = SECOND_DATA + 16;

        let mut elf = [0u8; IMAGE_SIZE];
        let original = sample();

        elf[..original.len()].copy_from_slice(&original);
        write_u16(&mut elf, header_field::PROGRAM_COUNT, 2);

        write_u32(&mut elf, SECOND_HEADER + segment_field::TYPE, PT_LOAD);
        write_u32(&mut elf, SECOND_HEADER + segment_field::FLAGS, PF_READ);
        write_u64(
            &mut elf,
            SECOND_HEADER + segment_field::FILE_OFFSET,
            SECOND_DATA as u64,
        );
        write_u64(
            &mut elf,
            SECOND_HEADER + segment_field::VIRTUAL_ADDRESS,
            0x4000,
        );
        write_u64(&mut elf, SECOND_HEADER + segment_field::FILE_SIZE, 16);
        write_u64(&mut elf, SECOND_HEADER + segment_field::MEMORY_SIZE, 16);
        write_u64(&mut elf, SECOND_HEADER + segment_field::ALIGNMENT, 1);

        let plan = plan_memory(&elf, elf.len() as u64).unwrap();

        assert_eq!(plan.loadable_segments, 2);
        assert_eq!(plan.virtual_start, 0x1000);
        assert_eq!(plan.virtual_end, 0x5000);
        assert_eq!(plan.page_count, 4);
    }

    #[test]
    fn rejects_invalid_segments() {
        let mut elf = sample();

        write_u64(&mut elf, HEADER_SIZE + segment_field::MEMORY_SIZE, 1);

        assert_eq!(
            plan_memory(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_page_rounding_overflow() {
        let mut elf = sample();
        let start = u64::MAX - (PAGE_SIZE - 1);

        write_u64(
            &mut elf,
            HEADER_SIZE + segment_field::VIRTUAL_ADDRESS,
            start,
        );
        write_u64(&mut elf, header_field::ENTRY, start);
        write_u64(
            &mut elf,
            HEADER_SIZE + segment_field::MEMORY_SIZE,
            PAGE_SIZE / 2,
        );

        assert_eq!(
            plan_memory(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }
}
