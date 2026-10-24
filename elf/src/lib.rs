#![no_std]

#[derive(Debug, PartialEq, Eq)]
pub enum ElfError {
    InvalidMagic,
    UnsupportedFormat,
    MalformedHeader,
    Truncated,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ElfHeader {
    pub entry: u64,
    pub program_header_offset: usize,
    pub program_header_count: u16,
}

fn read<const N: usize>(data: &[u8], offset: usize) -> Option<[u8; N]> {
    let end = offset.checked_add(N)?;
    data.get(offset..end)?.try_into().ok()
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(read(data, offset)?))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(read(data, offset)?))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(read(data, offset)?))
}

pub fn parse_header(data: &[u8]) -> Result<ElfHeader, ElfError> {
    use ElfError::*;

    if data.len() < 64 {
        return Err(Truncated);
    }

    if &data[..4] != b"\x7FELF" {
        return Err(InvalidMagic);
    }

    // ELF64, little-endian, current ELF version.
    if data[4] != 2 || data[5] != 1 || data[6] != 1 {
        return Err(UnsupportedFormat);
    }

    // Executable, x86-64, current ELF version.
    if read_u16(data, 16) != Some(2)
        || read_u16(data, 18) != Some(62)
        || read_u32(data, 20) != Some(1)
    {
        return Err(UnsupportedFormat);
    }

    // Standard ELF64 header and program-header sizes.
    if read_u16(data, 52) != Some(64) || read_u16(data, 54) != Some(56) {
        return Err(MalformedHeader);
    }

    let entry = read_u64(data, 24).ok_or(Truncated)?;
    let offset = read_u64(data, 32).ok_or(Truncated)?;

    let offset = usize::try_from(offset).map_err(|_| MalformedHeader)?;

    let count = read_u16(data, 56).ok_or(Truncated)?;

    if entry == 0 || count == 0 || offset < 64 {
        return Err(MalformedHeader);
    }

    let table_size = usize::from(count).checked_mul(56).ok_or(MalformedHeader)?;

    let table_end = offset.checked_add(table_size).ok_or(MalformedHeader)?;

    if table_end > data.len() {
        return Err(Truncated);
    }

    Ok(ElfHeader {
        entry,
        program_header_offset: offset,
        program_header_count: count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> [u8; 120] {
        let mut elf = [0u8; 120];

        elf[..4].copy_from_slice(b"\x7FELF");
        elf[4..7].copy_from_slice(&[2, 1, 1]);
        elf[16..18].copy_from_slice(&2u16.to_le_bytes());
        elf[18..20].copy_from_slice(&62u16.to_le_bytes());
        elf[20..24].copy_from_slice(&1u32.to_le_bytes());
        elf[24..32].copy_from_slice(&0x1000u64.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[52..54].copy_from_slice(&64u16.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());

        elf
    }

    #[test]
    fn accepts_valid_header() {
        assert_eq!(parse_header(&sample()).unwrap().entry, 0x1000);
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
        elf[18] = 3;

        assert_eq!(parse_header(&elf), Err(ElfError::UnsupportedFormat));
    }

    #[test]
    fn rejects_truncated_program_table() {
        let elf = sample();

        assert_eq!(parse_header(&elf[..119]), Err(ElfError::Truncated));
    }
}
