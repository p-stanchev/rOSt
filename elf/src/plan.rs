use crate::header::{self, PROGRAM_HEADER_SIZE, parse_header};
use crate::segment::{self, PT_LOAD, validate_load_segments};
use crate::{ElfError, read_u16, read_u32, read_u64};

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

/// Visit each loadable segment after validating the complete image.
pub fn visit_load_segments(
    data: &[u8],
    file_size: u64,
    mut visit: impl FnMut(LoadSegment),
) -> Result<ImageType, ElfError> {
    use ElfError::*;

    // Reject an invalid image before exposing any segments.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::field as header_field;
    use crate::test_support::{sample, write_u16, write_u64};

    #[test]
    fn reports_executable_segments() {
        let elf = sample();
        let mut found = None;

        let image_type =
            visit_load_segments(&elf, elf.len() as u64, |segment| found = Some(segment)).unwrap();

        assert_eq!(image_type, ImageType::Executable);

        let segment = found.unwrap();
        assert_eq!(segment.virtual_address, 0x1000);
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

        write_u64(
            &mut elf,
            header::HEADER_SIZE + segment::field::FILE_SIZE,
            u64::MAX,
        );

        let mut visits = 0;

        let result = visit_load_segments(&elf, elf.len() as u64, |_| visits += 1);

        assert_eq!(result, Err(ElfError::MalformedSegment));
        assert_eq!(visits, 0);
    }
}
