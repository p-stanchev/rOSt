use crate::header::{PROGRAM_HEADER_SIZE, parse_header};
use crate::segment::{self, validate_load_segments};
use crate::{ElfError, read_u32, read_u64};

const PT_DYNAMIC: u32 = 2;
const DYNAMIC_ENTRY_SIZE: usize = 16;
const DYNAMIC_VALUE_OFFSET: usize = 8;

// ELF dynamic table tags.
const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_PLTRELSZ: u64 = 2;
const DT_RELASZ: u64 = 8;
const DT_RELSZ: u64 = 18;
const DT_TEXTREL: u64 = 22;
const DT_RELRSZ: u64 = 35;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DynamicInfo {
    pub present: bool,
    pub rela_bytes: u64,
    pub rel_bytes: u64,
    pub relr_bytes: u64,
    pub plt_bytes: u64,
    pub needed_libraries: u16,
    pub text_relocations: bool,
}

impl DynamicInfo {
    pub fn has_relocation_tables(&self) -> bool {
        self.rela_bytes != 0 || self.rel_bytes != 0 || self.relr_bytes != 0 || self.plt_bytes != 0
    }
}

/// Inspect relocation metadata without reading or applying relocations.
pub fn inspect_dynamic(data: &[u8], file_size: u64) -> Result<DynamicInfo, ElfError> {
    use ElfError::*;

    validate_load_segments(data, file_size)?;

    let header = parse_header(data)?;
    let mut info = DynamicInfo::default();

    for index in 0..usize::from(header.program_header_count) {
        let offset = header.program_header_offset + index * PROGRAM_HEADER_SIZE;

        let segment_type = read_u32(data, offset + segment::field::TYPE).ok_or(Truncated)?;

        if segment_type != PT_DYNAMIC {
            continue;
        }

        // A second dynamic table would make the layout ambiguous.
        if info.present {
            return Err(MalformedSegment);
        }

        info.present = true;

        let file_offset = read_u64(data, offset + segment::field::FILE_OFFSET).ok_or(Truncated)?;

        let byte_count = read_u64(data, offset + segment::field::FILE_SIZE).ok_or(Truncated)?;

        if byte_count == 0 || byte_count % DYNAMIC_ENTRY_SIZE as u64 != 0 {
            return Err(MalformedSegment);
        }

        let file_end = file_offset
            .checked_add(byte_count)
            .ok_or(MalformedSegment)?;

        if file_end > file_size {
            return Err(Truncated);
        }

        let start = usize::try_from(file_offset).map_err(|_| MalformedSegment)?;
        let end = usize::try_from(file_end).map_err(|_| Truncated)?;

        let table = data.get(start..end).ok_or(Truncated)?;
        let mut terminated = false;

        for entry in table.chunks_exact(DYNAMIC_ENTRY_SIZE) {
            let tag = read_u64(entry, 0).ok_or(Truncated)?;
            let value = read_u64(entry, DYNAMIC_VALUE_OFFSET).ok_or(Truncated)?;

            if tag == DT_NULL {
                terminated = true;
                break;
            }

            match tag {
                DT_RELASZ => info.rela_bytes = value,
                DT_RELSZ => info.rel_bytes = value,
                DT_RELRSZ => info.relr_bytes = value,
                DT_PLTRELSZ => info.plt_bytes = value,
                DT_TEXTREL => info.text_relocations = true,
                DT_NEEDED => {
                    info.needed_libraries = info
                        .needed_libraries
                        .checked_add(1)
                        .ok_or(MalformedSegment)?;
                }
                _ => {}
            }
        }

        if !terminated {
            return Err(MalformedSegment);
        }
    }

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{HEADER_SIZE, field as header_field};
    use crate::test_support::{TEST_ENTRY, sample, write_u16, write_u32, write_u64};

    const DYNAMIC_HEADER_OFFSET: usize = HEADER_SIZE + PROGRAM_HEADER_SIZE;

    const DYNAMIC_DATA_OFFSET: usize = DYNAMIC_HEADER_OFFSET + PROGRAM_HEADER_SIZE;

    const DYNAMIC_ENTRY_COUNT: usize = 4;
    const DYNAMIC_BYTES: usize = DYNAMIC_ENTRY_COUNT * DYNAMIC_ENTRY_SIZE;

    const IMAGE_SIZE: usize = DYNAMIC_DATA_OFFSET + DYNAMIC_BYTES;

    fn write_tag(data: &mut [u8], index: usize, tag: u64, value: u64) {
        let offset = DYNAMIC_DATA_OFFSET + index * DYNAMIC_ENTRY_SIZE;

        write_u64(data, offset, tag);
        write_u64(data, offset + DYNAMIC_VALUE_OFFSET, value);
    }

    // Add a dynamic table to our existing test executable.
    fn sample_with_dynamic() -> [u8; IMAGE_SIZE] {
        let mut elf = [0u8; IMAGE_SIZE];
        let original = sample();

        elf[..original.len()].copy_from_slice(&original);

        write_u16(&mut elf, header_field::PROGRAM_COUNT, 2);

        // Extend the loadable segment to cover the test metadata.
        write_u64(
            &mut elf,
            HEADER_SIZE + segment::field::FILE_SIZE,
            IMAGE_SIZE as u64,
        );
        write_u64(
            &mut elf,
            HEADER_SIZE + segment::field::MEMORY_SIZE,
            IMAGE_SIZE as u64,
        );

        write_u32(
            &mut elf,
            DYNAMIC_HEADER_OFFSET + segment::field::TYPE,
            PT_DYNAMIC,
        );
        write_u64(
            &mut elf,
            DYNAMIC_HEADER_OFFSET + segment::field::FILE_OFFSET,
            DYNAMIC_DATA_OFFSET as u64,
        );
        write_u64(
            &mut elf,
            DYNAMIC_HEADER_OFFSET + segment::field::VIRTUAL_ADDRESS,
            TEST_ENTRY + DYNAMIC_DATA_OFFSET as u64,
        );
        write_u64(
            &mut elf,
            DYNAMIC_HEADER_OFFSET + segment::field::FILE_SIZE,
            DYNAMIC_BYTES as u64,
        );
        write_u64(
            &mut elf,
            DYNAMIC_HEADER_OFFSET + segment::field::MEMORY_SIZE,
            DYNAMIC_BYTES as u64,
        );

        // A synthetic RELA table declaration.
        write_tag(&mut elf, 0, 7, TEST_ENTRY + DYNAMIC_DATA_OFFSET as u64);
        write_tag(&mut elf, 1, DT_RELASZ, 24);
        write_tag(&mut elf, 2, 9, 24);
        write_tag(&mut elf, 3, DT_NULL, 0);

        elf
    }

    #[test]
    fn accepts_image_without_dynamic_segment() {
        let elf = sample();

        assert_eq!(
            inspect_dynamic(&elf, elf.len() as u64),
            Ok(DynamicInfo::default())
        );
    }

    #[test]
    fn reports_rela_metadata() {
        let elf = sample_with_dynamic();
        let info = inspect_dynamic(&elf, elf.len() as u64).unwrap();

        assert!(info.present);
        assert_eq!(info.rela_bytes, 24);
        assert!(info.has_relocation_tables());
    }

    #[test]
    fn reports_relr_metadata() {
        let mut elf = sample_with_dynamic();

        write_tag(&mut elf, 0, 36, TEST_ENTRY);
        write_tag(&mut elf, 1, DT_RELRSZ, 16);

        let info = inspect_dynamic(&elf, elf.len() as u64).unwrap();

        assert_eq!(info.relr_bytes, 16);
        assert!(info.has_relocation_tables());
    }

    #[test]
    fn reports_required_libraries() {
        let mut elf = sample_with_dynamic();

        write_tag(&mut elf, 0, DT_NEEDED, 1);
        write_tag(&mut elf, 1, DT_NULL, 0);

        let info = inspect_dynamic(&elf, elf.len() as u64).unwrap();

        assert_eq!(info.needed_libraries, 1);
        assert!(!info.has_relocation_tables());
    }

    #[test]
    fn reports_text_relocations() {
        let mut elf = sample_with_dynamic();

        write_tag(&mut elf, 2, DT_TEXTREL, 0);

        let info = inspect_dynamic(&elf, elf.len() as u64).unwrap();

        assert!(info.text_relocations);
    }

    #[test]
    fn rejects_missing_terminator() {
        let mut elf = sample_with_dynamic();

        write_tag(&mut elf, 3, DT_NEEDED, 1);

        assert_eq!(
            inspect_dynamic(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_truncated_dynamic_table() {
        let elf = sample_with_dynamic();

        assert_eq!(
            inspect_dynamic(
                &elf[..DYNAMIC_DATA_OFFSET + DYNAMIC_ENTRY_SIZE],
                elf.len() as u64,
            ),
            Err(ElfError::Truncated)
        );
    }

    #[test]
    fn rejects_invalid_dynamic_table_size() {
        let mut elf = sample_with_dynamic();

        write_u64(
            &mut elf,
            DYNAMIC_HEADER_OFFSET + segment::field::FILE_SIZE,
            (DYNAMIC_BYTES - 1) as u64,
        );

        assert_eq!(
            inspect_dynamic(&elf, elf.len() as u64),
            Err(ElfError::MalformedSegment)
        );
    }

    #[test]
    fn rejects_dynamic_table_outside_file() {
        let mut elf = sample_with_dynamic();

        write_u64(
            &mut elf,
            DYNAMIC_HEADER_OFFSET + segment::field::FILE_OFFSET,
            IMAGE_SIZE as u64,
        );

        assert_eq!(
            inspect_dynamic(&elf, elf.len() as u64),
            Err(ElfError::Truncated)
        );
    }
}
