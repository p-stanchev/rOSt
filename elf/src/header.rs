use crate::{ElfError, read_u16, read_u32, read_u64};

pub(crate) const HEADER_SIZE: usize = 64;
pub(crate) const PROGRAM_HEADER_SIZE: usize = 56;

pub(crate) const ELF_MAGIC: [u8; 4] = *b"\x7FELF";
pub(crate) const ELF_CLASS_64: u8 = 2;
pub(crate) const ELF_DATA_LITTLE_ENDIAN: u8 = 1;
pub(crate) const ELF_VERSION: u32 = 1;

pub(crate) const ET_EXEC: u16 = 2;
pub(crate) const ET_DYN: u16 = 3;
pub(crate) const EM_X86_64: u16 = 62;

// Byte positions of the fields in an ELF64 header.
pub(crate) mod field {
    pub const CLASS: usize = 4;
    pub const DATA: usize = 5;
    pub const IDENT_VERSION: usize = 6;
    pub const FILE_TYPE: usize = 16;
    pub const MACHINE: usize = 18;
    pub const VERSION: usize = 20;
    pub const ENTRY: usize = 24;
    pub const PROGRAM_OFFSET: usize = 32;
    pub const HEADER_SIZE: usize = 52;
    pub const PROGRAM_ENTRY_SIZE: usize = 54;
    pub const PROGRAM_COUNT: usize = 56;
}

#[derive(Debug, PartialEq, Eq)]
pub struct ElfHeader {
    pub entry: u64,
    pub program_header_offset: usize,
    pub program_header_count: u16,
}

/// Check an ELF64 header and locate its program-header table.
pub fn parse_header(data: &[u8]) -> Result<ElfHeader, ElfError> {
    use ElfError::*;

    if data.len() < HEADER_SIZE {
        return Err(Truncated);
    }

    if !data.starts_with(&ELF_MAGIC) {
        return Err(InvalidMagic);
    }

    if data[field::CLASS] != ELF_CLASS_64
        || data[field::DATA] != ELF_DATA_LITTLE_ENDIAN
        || data[field::IDENT_VERSION] != ELF_VERSION as u8
    {
        return Err(UnsupportedFormat);
    }

    let file_type = read_u16(data, field::FILE_TYPE).ok_or(Truncated)?;
    let machine = read_u16(data, field::MACHINE).ok_or(Truncated)?;
    let version = read_u32(data, field::VERSION).ok_or(Truncated)?;

    if !matches!(file_type, ET_EXEC | ET_DYN) || machine != EM_X86_64 || version != ELF_VERSION {
        return Err(UnsupportedFormat);
    }

    if read_u16(data, field::HEADER_SIZE) != Some(HEADER_SIZE as u16)
        || read_u16(data, field::PROGRAM_ENTRY_SIZE) != Some(PROGRAM_HEADER_SIZE as u16)
    {
        return Err(MalformedHeader);
    }

    let entry = read_u64(data, field::ENTRY).ok_or(Truncated)?;
    let table_offset = read_u64(data, field::PROGRAM_OFFSET).ok_or(Truncated)?;
    let table_offset = usize::try_from(table_offset).map_err(|_| MalformedHeader)?;
    let table_count = read_u16(data, field::PROGRAM_COUNT).ok_or(Truncated)?;

    if entry == 0 || table_count == 0 || table_offset < HEADER_SIZE {
        return Err(MalformedHeader);
    }

    // Verify that the complete table fits inside the supplied data.
    let table_bytes = usize::from(table_count)
        .checked_mul(PROGRAM_HEADER_SIZE)
        .ok_or(MalformedHeader)?;

    let table_end = table_offset
        .checked_add(table_bytes)
        .ok_or(MalformedHeader)?;

    if table_end > data.len() {
        return Err(Truncated);
    }

    Ok(ElfHeader {
        entry,
        program_header_offset: table_offset,
        program_header_count: table_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TEST_ENTRY, sample, write_u16, write_u64};

    #[test]
    fn accepts_valid_header() {
        assert_eq!(parse_header(&sample()).unwrap().entry, TEST_ENTRY);
    }

    #[test]
    fn accepts_position_independent_executable() {
        let mut elf = sample();
        write_u16(&mut elf, field::FILE_TYPE, ET_DYN);

        assert!(parse_header(&elf).is_ok());
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut elf = sample();
        elf[0] = 0;

        assert_eq!(parse_header(&elf), Err(ElfError::InvalidMagic));
    }

    #[test]
    fn rejects_wrong_architecture() {
        let mut elf = sample();
        write_u16(&mut elf, field::MACHINE, 0);

        assert_eq!(parse_header(&elf), Err(ElfError::UnsupportedFormat));
    }

    #[test]
    fn rejects_truncated_program_table() {
        let elf = sample();

        assert_eq!(
            parse_header(&elf[..elf.len() - 1]),
            Err(ElfError::Truncated)
        );
    }

    #[test]
    fn rejects_invalid_program_table_offset() {
        let mut elf = sample();
        write_u64(&mut elf, field::PROGRAM_OFFSET, u64::MAX);

        assert!(matches!(
            parse_header(&elf),
            Err(ElfError::Truncated | ElfError::MalformedHeader)
        ));
    }
}
