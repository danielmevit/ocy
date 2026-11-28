//! Reads the executable's CodeView (RSDS) PDB signature (`GUID-age`) from PE debug metadata.
//!
//! Used on Windows so crash reports record the exact build ID matching the archived `.pdb`.
//! Pure parsing helpers operate on `&[u8]` buffers and are unit-tested on all platforms.

use std::fmt;

const IMAGE_DOS_SIGNATURE: u16 = 0x5A4D; // "MZ"
const IMAGE_NT_SIGNATURE: u32 = 0x0000_4550; // "PE\0\0"
const IMAGE_NT_OPTIONAL_HDR32_MAGIC: u16 = 0x010B;
const IMAGE_NT_OPTIONAL_HDR64_MAGIC: u16 = 0x020B;
const IMAGE_DIRECTORY_ENTRY_DEBUG: usize = 6;
const IMAGE_DEBUG_TYPE_CODEVIEW: u32 = 2;
const CODEVIEW_RSDS_SIGNATURE: &[u8; 4] = b"RSDS";
const DEBUG_DIR_ENTRY_SIZE: usize = 28;
const SECTION_HEADER_SIZE: usize = 40;
const MAX_DEBUG_ENTRIES: usize = 64;
const MAX_SECTIONS: usize = 96;

/// Parsed CodeView RSDS PDB identifier from a PE debug directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdbSignature {
    /// Raw 16-byte Windows GUID (`Data1` LE, `Data2` LE, `Data3` LE, `Data4` BE).
    pub guid: [u8; 16],
    /// Incremental link age (`1` for a clean build).
    pub age: u32,
    /// PDB filename/path embedded in the RSDS record.
    pub pdb_name: String,
}

impl PdbSignature {
    /// Formats the 16-byte Windows GUID in canonical uppercase `8-4-4-4-12` hex form.
    #[must_use]
    pub fn format_guid(&self) -> String {
        let d1 = u32::from_le_bytes([self.guid[0], self.guid[1], self.guid[2], self.guid[3]]);
        let d2 = u16::from_le_bytes([self.guid[4], self.guid[5]]);
        let d3 = u16::from_le_bytes([self.guid[6], self.guid[7]]);
        format!(
            "{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            self.guid[8],
            self.guid[9],
            self.guid[10],
            self.guid[11],
            self.guid[12],
            self.guid[13],
            self.guid[14],
            self.guid[15]
        )
    }

    /// Formats the PDB signature as `GUID-age` (e.g. `12345678-90AB-CDEF-1122-334455667788-1`).
    #[must_use]
    pub fn format_id(&self) -> String {
        format!("{}-{}", self.format_guid(), self.age)
    }
}

impl fmt::Display for PdbSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format_id())
    }
}

/// Returns the running executable's CodeView PDB signature (`GUID-age`), or `None` on non-Windows
/// platforms or when no CodeView RSDS record is present.
#[must_use]
pub fn current_pdb_build_id() -> Option<String> {
    #[cfg(windows)]
    {
        current_windows_pdb_build_id()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Parses a CodeView RSDS (`CV_INFO_PDB70`) debug record into a [`PdbSignature`].
#[must_use]
pub fn parse_codeview_rsds(record: &[u8]) -> Option<PdbSignature> {
    if record.len() < 24 || record.get(0..4)? != CODEVIEW_RSDS_SIGNATURE {
        return None;
    }
    let guid: [u8; 16] = record.get(4..20)?.try_into().ok()?;
    let age = read_u32_le(record, 20)?;
    let raw_name = record.get(24..)?.split(|&b| b == 0).next().unwrap_or(&[]);
    let pdb_name = String::from_utf8_lossy(raw_name).into_owned();
    Some(PdbSignature {
        guid,
        age,
        pdb_name,
    })
}

/// Parses the first CodeView RSDS [`PdbSignature`] from an unmapped on-disk PE file buffer.
#[must_use]
pub fn parse_pdb_signature_from_pe_file(pe_bytes: &[u8]) -> Option<PdbSignature> {
    parse_pdb_signature_from_pe(pe_bytes, false)
}

/// Parses `GUID-age` from an unmapped on-disk PE file buffer.
#[must_use]
pub fn parse_pdb_id_from_pe_file(pe_bytes: &[u8]) -> Option<String> {
    parse_pdb_signature_from_pe_file(pe_bytes).map(|sig| sig.format_id())
}

/// Parses the first CodeView RSDS [`PdbSignature`] from a memory-mapped PE image buffer.
#[must_use]
pub fn parse_pdb_signature_from_mapped_image(image: &[u8]) -> Option<PdbSignature> {
    parse_pdb_signature_from_pe(image, true)
}

/// Parses `GUID-age` from a memory-mapped PE image buffer.
#[must_use]
pub fn parse_pdb_id_from_mapped_image(image: &[u8]) -> Option<String> {
    parse_pdb_signature_from_mapped_image(image).map(|sig| sig.format_id())
}

fn parse_pdb_signature_from_pe(bytes: &[u8], mapped: bool) -> Option<PdbSignature> {
    let headers = parse_pe_headers(bytes)?;
    let debug_offset = resolve_rva(
        bytes,
        &headers,
        headers.debug_rva,
        headers.debug_size,
        mapped,
    )?;
    let debug_slice = bytes.get(debug_offset..debug_offset.checked_add(headers.debug_size)?)?;
    let entry_count = (debug_slice.len() / DEBUG_DIR_ENTRY_SIZE).min(MAX_DEBUG_ENTRIES);

    for idx in 0..entry_count {
        let entry_off = idx * DEBUG_DIR_ENTRY_SIZE;
        let debug_type = read_u32_le(debug_slice, entry_off + 12)?;
        if debug_type != IMAGE_DEBUG_TYPE_CODEVIEW {
            continue;
        }
        let size_of_data = read_u32_le(debug_slice, entry_off + 16)? as usize;
        let addr_of_raw = read_u32_le(debug_slice, entry_off + 20)?;
        let ptr_to_raw = read_u32_le(debug_slice, entry_off + 24)? as usize;

        if size_of_data < 24 {
            continue;
        }

        let data_offset = if mapped {
            if addr_of_raw == 0 {
                continue;
            }
            addr_of_raw as usize
        } else if ptr_to_raw != 0 {
            ptr_to_raw
        } else if addr_of_raw != 0 {
            resolve_rva(bytes, &headers, addr_of_raw, size_of_data, false)?
        } else {
            continue;
        };

        let Some(cv_slice) = bytes.get(data_offset..data_offset.checked_add(size_of_data)?) else {
            continue;
        };
        if let Some(sig) = parse_codeview_rsds(cv_slice) {
            return Some(sig);
        }
    }
    None
}

struct PeHeaders {
    num_sections: usize,
    sections_off: usize,
    #[cfg_attr(not(windows), allow(dead_code))]
    size_of_image: usize,
    debug_rva: u32,
    debug_size: usize,
}

fn parse_pe_headers(bytes: &[u8]) -> Option<PeHeaders> {
    if bytes.len() < 64 || read_u16_le(bytes, 0)? != IMAGE_DOS_SIGNATURE {
        return None;
    }
    let e_lfanew = read_u32_le(bytes, 0x3C)? as usize;
    if e_lfanew < 64 || read_u32_le(bytes, e_lfanew)? != IMAGE_NT_SIGNATURE {
        return None;
    }

    let file_header_off = e_lfanew.checked_add(4)?;
    let num_sections = read_u16_le(bytes, file_header_off + 2)? as usize;
    let size_of_opt_header = read_u16_le(bytes, file_header_off + 16)? as usize;
    let opt_off = file_header_off.checked_add(20)?;
    let sections_off = opt_off.checked_add(size_of_opt_header)?;

    let magic = read_u16_le(bytes, opt_off)?;
    let (num_rva_rel, data_dir_rel) = match magic {
        IMAGE_NT_OPTIONAL_HDR32_MAGIC => (92_usize, 96_usize),
        IMAGE_NT_OPTIONAL_HDR64_MAGIC => (108_usize, 112_usize),
        _ => return None,
    };

    let min_opt_for_debug = data_dir_rel + (IMAGE_DIRECTORY_ENTRY_DEBUG + 1) * 8;
    if size_of_opt_header < min_opt_for_debug {
        return None;
    }

    let size_of_image = read_u32_le(bytes, opt_off + 56)? as usize;
    let num_rva_and_sizes = read_u32_le(bytes, opt_off.checked_add(num_rva_rel)?)? as usize;
    if num_rva_and_sizes <= IMAGE_DIRECTORY_ENTRY_DEBUG {
        return None;
    }

    let debug_entry_off = opt_off
        .checked_add(data_dir_rel)?
        .checked_add(IMAGE_DIRECTORY_ENTRY_DEBUG * 8)?;
    let debug_rva = read_u32_le(bytes, debug_entry_off)?;
    let debug_size = read_u32_le(bytes, debug_entry_off + 4)? as usize;
    if debug_rva == 0 || debug_size < DEBUG_DIR_ENTRY_SIZE {
        return None;
    }

    Some(PeHeaders {
        num_sections,
        sections_off,
        size_of_image,
        debug_rva,
        debug_size,
    })
}

fn resolve_rva(
    bytes: &[u8],
    headers: &PeHeaders,
    rva: u32,
    len: usize,
    mapped: bool,
) -> Option<usize> {
    if mapped {
        let start = rva as usize;
        let end = start.checked_add(len)?;
        return (end <= bytes.len()).then_some(start);
    }

    let section_count = headers.num_sections.min(MAX_SECTIONS);
    for idx in 0..section_count {
        let sec_off = headers
            .sections_off
            .checked_add(idx * SECTION_HEADER_SIZE)?;
        if sec_off.checked_add(SECTION_HEADER_SIZE)? > bytes.len() {
            break;
        }
        let virt_size = read_u32_le(bytes, sec_off + 8)?;
        let virt_addr = read_u32_le(bytes, sec_off + 12)?;
        let raw_size = read_u32_le(bytes, sec_off + 16)?;
        let raw_ptr = read_u32_le(bytes, sec_off + 20)?;

        let span = virt_size.max(raw_size);
        if rva < virt_addr {
            continue;
        }
        let delta = rva - virt_addr;
        if delta >= span || delta >= raw_size {
            continue;
        }
        let file_off = (raw_ptr as usize).checked_add(delta as usize)?;
        let raw_end = (raw_ptr as usize).checked_add(raw_size as usize)?;
        let req_end = file_off.checked_add(len)?;
        if req_end <= raw_end && req_end <= bytes.len() {
            return Some(file_off);
        }
    }
    None
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice: [u8; 2] = bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
    Some(u16::from_le_bytes(slice))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(slice))
}

#[cfg(windows)]
fn current_windows_pdb_build_id() -> Option<String> {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::core::PCWSTR;

    const FIRST_PAGE_BYTES: usize = 4096;
    const MAX_REASONABLE_IMAGE_BYTES: usize = 256 * 1024 * 1024;

    // SAFETY: `GetModuleHandleW(None)` retrieves the base address of the current process's
    // main executable module, which remains mapped in memory for the lifetime of the process.
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?;
    let base = module.0 as *const u8;
    if base.is_null() {
        return None;
    }

    // SAFETY: The Windows PE loader always commits at least the initial 4 KiB header page of a
    // loaded executable image as readable memory starting at `base`.
    let header_page = unsafe { std::slice::from_raw_parts(base, FIRST_PAGE_BYTES) };
    let headers = parse_pe_headers(header_page)?;
    if headers.size_of_image < FIRST_PAGE_BYTES
        || headers.size_of_image > MAX_REASONABLE_IMAGE_BYTES
    {
        return None;
    }

    // SAFETY: `headers.size_of_image` was validated from the live PE OptionalHeader of this
    // process's main executable (`SEC_IMAGE` mapping), so `[base .. base + SizeOfImage)` is
    // mapped and valid for read-only inspection for the lifetime of the process.
    let image = unsafe { std::slice::from_raw_parts(base, headers.size_of_image) };
    parse_pdb_id_from_mapped_image(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u16(buf: &mut [u8], off: usize, val: u16) {
        buf[off..off + 2].copy_from_slice(&val.to_le_bytes());
    }

    fn put_u32(buf: &mut [u8], off: usize, val: u32) {
        buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
    }

    fn sample_rsds_record() -> Vec<u8> {
        let mut rec = Vec::new();
        rec.extend_from_slice(b"RSDS");
        // GUID: 12345678-90AB-CDEF-1122-334455667788
        rec.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
        rec.extend_from_slice(&0x90AB_u16.to_le_bytes());
        rec.extend_from_slice(&0xCDEF_u16.to_le_bytes());
        rec.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        // Age = 1
        rec.extend_from_slice(&1_u32.to_le_bytes());
        rec.extend_from_slice(b"Ocy.pdb\0");
        rec
    }

    fn build_synthetic_pe(mapped: bool, pe32_plus: bool, include_rsds: bool) -> Vec<u8> {
        let rsds = sample_rsds_record();
        let e_lfanew = 0x80_usize;
        let opt_size = if pe32_plus { 240_usize } else { 224_usize };
        let data_dir_rel = if pe32_plus { 112_usize } else { 96_usize };
        let num_rva_rel = if pe32_plus { 108_usize } else { 92_usize };
        let opt_off = e_lfanew + 24;
        let sec_off = opt_off + opt_size;

        let rdata_rva = 0x1000_u32;
        let rdata_raw = 0x400_u32;
        let debug_dir_rel_in_sec = 0_u32;
        let rsds_rel_in_sec = 0x40_u32;

        let total_size = if mapped { 0x2000 } else { 0x800 };
        let mut buf = vec![0_u8; total_size];

        // DOS header
        put_u16(&mut buf, 0, IMAGE_DOS_SIGNATURE);
        put_u32(&mut buf, 0x3C, e_lfanew as u32);

        // NT signature + FileHeader (1 section)
        put_u32(&mut buf, e_lfanew, IMAGE_NT_SIGNATURE);
        put_u16(&mut buf, e_lfanew + 6, 1);
        put_u16(&mut buf, e_lfanew + 20, opt_size as u16);

        // OptionalHeader
        put_u16(
            &mut buf,
            opt_off,
            if pe32_plus {
                IMAGE_NT_OPTIONAL_HDR64_MAGIC
            } else {
                IMAGE_NT_OPTIONAL_HDR32_MAGIC
            },
        );
        put_u32(&mut buf, opt_off + 56, 0x2000); // SizeOfImage
        put_u32(&mut buf, opt_off + num_rva_rel, 16);

        // Debug data directory (index 6)
        let debug_rva = rdata_rva + debug_dir_rel_in_sec;
        put_u32(&mut buf, opt_off + data_dir_rel + 6 * 8, debug_rva);
        put_u32(
            &mut buf,
            opt_off + data_dir_rel + 6 * 8 + 4,
            (DEBUG_DIR_ENTRY_SIZE * 2) as u32,
        );

        // Section ".rdata"
        buf[sec_off..sec_off + 6].copy_from_slice(b".rdata");
        put_u32(&mut buf, sec_off + 8, 0x200); // VirtualSize
        put_u32(&mut buf, sec_off + 12, rdata_rva);
        put_u32(&mut buf, sec_off + 16, 0x200); // SizeOfRawData
        put_u32(&mut buf, sec_off + 20, rdata_raw);

        let sec_base = if mapped {
            rdata_rva as usize
        } else {
            rdata_raw as usize
        };

        // Entry 0: non-CodeView (POGO = 13) so loop must skip it
        put_u32(&mut buf, sec_base + 12, 13);
        put_u32(&mut buf, sec_base + 16, 4);

        // Entry 1: CodeView
        let cv_entry = sec_base + DEBUG_DIR_ENTRY_SIZE;
        put_u32(&mut buf, cv_entry + 12, IMAGE_DEBUG_TYPE_CODEVIEW);
        put_u32(&mut buf, cv_entry + 16, rsds.len() as u32);
        put_u32(&mut buf, cv_entry + 20, rdata_rva + rsds_rel_in_sec);
        put_u32(&mut buf, cv_entry + 24, rdata_raw + rsds_rel_in_sec);

        let rsds_off = sec_base + rsds_rel_in_sec as usize;
        if include_rsds {
            buf[rsds_off..rsds_off + rsds.len()].copy_from_slice(&rsds);
        } else {
            buf[rsds_off..rsds_off + 4].copy_from_slice(b"NB10");
        }

        buf
    }

    #[test]
    fn parses_pe32_plus_file_and_mapped_image() {
        let unmapped = build_synthetic_pe(false, true, true);
        let mapped = build_synthetic_pe(true, true, true);

        let sig_file = parse_pdb_signature_from_pe_file(&unmapped).expect("file signature");
        assert_eq!(
            sig_file.format_id(),
            "12345678-90AB-CDEF-1122-334455667788-1"
        );
        assert_eq!(sig_file.pdb_name, "Ocy.pdb");

        assert_eq!(
            parse_pdb_id_from_pe_file(&unmapped).as_deref(),
            Some("12345678-90AB-CDEF-1122-334455667788-1")
        );
        assert_eq!(
            parse_pdb_id_from_mapped_image(&mapped).as_deref(),
            Some("12345678-90AB-CDEF-1122-334455667788-1")
        );
    }

    #[test]
    fn parses_pe32_file() {
        let pe32 = build_synthetic_pe(false, false, true);
        assert_eq!(
            parse_pdb_id_from_pe_file(&pe32).as_deref(),
            Some("12345678-90AB-CDEF-1122-334455667788-1")
        );
    }

    #[test]
    fn rejects_corrupt_or_missing_codeview() {
        assert_eq!(parse_pdb_id_from_pe_file(&[]), None);
        assert_eq!(parse_pdb_id_from_pe_file(b"NOT_A_PE"), None);
        assert_eq!(parse_codeview_rsds(b"RSDSshort"), None);

        let no_rsds = build_synthetic_pe(false, true, false);
        assert_eq!(parse_pdb_id_from_pe_file(&no_rsds), None);

        let mut truncated = build_synthetic_pe(true, true, true);
        truncated.truncate(0x100);
        assert_eq!(parse_pdb_id_from_mapped_image(&truncated), None);
    }

    #[test]
    fn handles_multi_digit_age() {
        let mut rec = sample_rsds_record();
        rec[20..24].copy_from_slice(&14_u32.to_le_bytes());
        let sig = parse_codeview_rsds(&rec).expect("valid RSDS");
        assert_eq!(sig.format_id(), "12345678-90AB-CDEF-1122-334455667788-14");
    }
}
