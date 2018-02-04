// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate error_chain;

mod errors {
    error_chain!{
        errors { NameError }
    }
}
use errors::{Error, ErrorKind, Result, ResultExt};

use std::{cmp, mem, str};
use std::collections::HashMap;

pub struct PE {
    // Maps from vaddr (as we may see in CODE) to the function name to thunk to.
    pub thunks: Option<HashMap<u32, Thunk>>,

    // A list of offsets in CODE containing a 32bit address that needs to be relocated in memory.
    // The base is always 0, so just add the address of CODE.
    pub relocs: Vec<u32>,

    // The code itself, copied out of the source.
    pub code: Vec<u8>,

    // The data length for assertion checks.
    pub code_vaddr: u32,
}

#[derive(Debug, Clone)]
pub struct Thunk {
    pub name: String,  // The name of this import.
    pub ordinal: u32,  // The ordinal of this import.
    pub vaddr: u32,  // Virtual address of the thunk of this symbol.
}

impl PE {
    pub fn parse(data: &[u8]) -> Result<PE> {
        assert_eq!(mem::size_of::<COFFHeader>(), 20);
        assert_eq!(mem::size_of::<OptionalHeader>(), 28);
        assert_eq!(mem::size_of::<WindowsHeader>(), 68);

        ensure!(data.len() > 0x3c + 4, "pe file too short for dos header");
        ensure!(data[0] == 0x4d && data[1] == 0x5a, "not a dos program file header");
        let pe_offset_ptr: *const u32 = data[0x3c..].as_ptr() as *const _;
        let pe_offset = unsafe { *pe_offset_ptr } as usize;

        ensure!(data.len() > pe_offset + 4 + 20 + 28, "pe file to short for coff headers");
        ensure!(data[pe_offset] as char == 'P' && data[pe_offset + 1] as char == 'L', "did not find pe header");
        let coff_offset = pe_offset + 4;
        let coff_ptr: *const COFFHeader = data[coff_offset..].as_ptr() as *const _;
        let coff: &COFFHeader = unsafe { &*coff_ptr };
        ensure!(coff.machine == 0x14C, "expected i386 machine type field");
        ensure!(coff.characteristics == 0xA18E, "expected a specific set of coff flags");
        ensure!(coff.pointer_to_symbol_table == 0, "expected nil symbol table");
        ensure!(coff.number_of_symbols == 0, "expected no symbols");
        ensure!(coff.size_of_optional_header == 224, "a normal PE file");

        let opt_offset = pe_offset + 4 + mem::size_of::<COFFHeader>();
        let opt_ptr: *const OptionalHeader = data[opt_offset..].as_ptr() as *const _;
        let opt: &OptionalHeader = unsafe { &*opt_ptr };
        ensure!(opt.magic == 0x10B, "expected a PE optional header magic");
        ensure!(opt.size_of_uninitialized_data == 0, "expected no uninitialized data");
        ensure!(opt.address_of_entry_point == 0, "expected entry to be at zero");
        ensure!(opt.base_of_code == 4096, "expected code to live at page 1");
        // opt.size_of_code
        // opt.size_of_initialize_data
        // opt.base_of_data

        let win_offset = pe_offset + 4 + mem::size_of::<COFFHeader>() + mem::size_of::<OptionalHeader>();
        let win_ptr: *const WindowsHeader = data[win_offset..].as_ptr() as *const _;
        let win: &WindowsHeader = unsafe { &*win_ptr };
        ensure!(win.image_base == 0, "expected image base to be zero");
        ensure!(win.section_alignment == 4096, "expected page aligned memory sections");
        ensure!(win.file_alignment == 512, "expected block aligned file sections");
        ensure!(win.major_image_version == 0, "major image version should be unset");
        ensure!(win.minor_image_version == 0, "minor image version should be unset");
        ensure!(win.win32_version_value == 0, "win32 version version should be unset");
        ensure!(win.size_of_headers == 1024, "expected exactly 1K of headers");
        ensure!(win.checksum == 0, "checksum should be unset");
        ensure!(win.subsystem == 66, "subsystem must be exactly 66");
        ensure!(win.dll_characteristics == 0, "dll_characteristics should be unset");
        ensure!(win.size_of_stack_reserve == 0, "stack not supported");
        ensure!(win.size_of_stack_commit == 0, "stack not supported");
        ensure!(win.loader_flags == 0, "loader flags should not be set");
        // win.size_of_heap_reserve = 1M
        // win.size_of_heap_commit = 4096
        // win.number_of_rvas_and_sizes == 16
        // win.size_of_image

        // Note: we skip the directory data because the section labels have reliably correct names.

        let section_table_offset = pe_offset + 4 + mem::size_of::<COFFHeader>() + coff.size_of_optional_header as usize;
        let mut sections = HashMap::new();
        for i in 0..coff.number_of_sections as usize {
            let section_offset = section_table_offset + i * mem::size_of::<SectionHeader>();
            let section_ptr: *const SectionHeader = data[section_offset..].as_ptr() as *const _;
            let section: &SectionHeader = unsafe { &*section_ptr };
            ensure!(section.number_of_relocations == 0, "relocations are not supported");
            ensure!(section.pointer_to_relocations == 0, "relocations are not supported");
            ensure!(section.number_of_line_numbers == 0, "line numbers are not supported");
            ensure!(section.pointer_to_line_numbers == 0, "line numbers are not supported");

            let name_raw = match section.name.iter().position(|&n| n == 0u8) {
                Some(last) => &section.name[..last],
                None => &section.name[0..8],
            };
            let name = str::from_utf8(name_raw).chain_err(|| "invalid section name")?;

            let expect_flags = match name {
                "CODE" => SectionFlags::IMAGE_SCN_CNT_CODE | SectionFlags::IMAGE_SCN_MEM_EXECUTE | SectionFlags::IMAGE_SCN_MEM_READ | SectionFlags::IMAGE_SCN_MEM_WRITE,
                ".idata" => SectionFlags::IMAGE_SCN_CNT_INITIALIZED_DATA | SectionFlags::IMAGE_SCN_MEM_READ | SectionFlags::IMAGE_SCN_MEM_WRITE,
                ".reloc" => SectionFlags::IMAGE_SCN_CNT_INITIALIZED_DATA | SectionFlags::IMAGE_SCN_MEM_DISCARDABLE | SectionFlags::IMAGE_SCN_MEM_READ,
                "$$DOSX" => SectionFlags::IMAGE_SCN_CNT_INITIALIZED_DATA | SectionFlags::IMAGE_SCN_MEM_DISCARDABLE | SectionFlags::IMAGE_SCN_MEM_READ,
                s => bail!("unexpected section name: {}", s)
            };
            ensure!(SectionFlags::from_u32(section.characteristics) == expect_flags, "unexpected section flags");

            // println!("Section {} starting at offset {:X} loaded at vaddr {:X}", name, section.pointer_to_raw_data, section.virtual_address);
            let start = section.pointer_to_raw_data as usize;
            let end = start + section.virtual_size as usize;
            let section_data = &data[start..end];
            if name == "$$DOSX" {
                ensure!(section_data == DOSX_HEADER, "expected a fixed-content DOSX header");
                continue;
            }

            sections.insert(name, (section, section_data));
        }

        let thunks = match sections.contains_key(".idata") {
            true => {
                let (idata_section, idata) = sections[".idata"];
                Some(PE::_parse_idata(idata_section, idata).chain_err(|| "parse idata")?)
            },
            false => None
        };

        let (code_section, code) = sections["CODE"];
        let (_, reloc_data) = sections[".reloc"];
        let relocs = PE::_parse_relocs(reloc_data, code_section).chain_err(|| "parse relocs")?;

        return Ok(PE { thunks, relocs, code: code.to_owned(), code_vaddr: code_section.virtual_address});
    }

    fn _parse_idata(section: &SectionHeader, idata: &[u8]) -> Result<HashMap<u32, Thunk>> {
        ensure!(idata.len() > mem::size_of::<ImportDirectoryEntry>() * 2, "section data too short for directory");

        // Assert that there is exactly one entry by loading the second section and checking
        // that it is null.
        let term_ptr: *const ImportDirectoryEntry = idata[mem::size_of::<ImportDirectoryEntry>()..].as_ptr() as *const _;
        let term: &ImportDirectoryEntry = unsafe { &*term_ptr };
        ensure!(term.import_lut_rva == 0 && term.timestamp == 0 && term.forwarder_chain == 0 && term.name_rva == 0 && term.thunk_table == 0, "expected one import dirctory entry");

        let dir_ptr: *const ImportDirectoryEntry = idata.as_ptr() as *const _;
        let dir: &ImportDirectoryEntry = unsafe { &*dir_ptr };

        // Check that the name is main.dll.
        ensure!(dir.name_rva > section.virtual_address, "dll name not in section");
        ensure!(dir.name_rva < section.virtual_address + section.virtual_size, "dll name not in section");
        let dll_name_offset = dir.name_rva as usize - section.virtual_address as usize;
        let dll_name = Self::read_name(&idata[dll_name_offset..]).chain_err(|| "dll name")?;
        ensure!(dll_name == "main.dll", "expected the directory entry to be for main.dll");

        // Iterate the name/thunk tables in parallel, extracting vaddr and name mappings.
        let lut_offset = dir.import_lut_rva as usize - section.virtual_address as usize;
        let thunk_offset = dir.thunk_table as usize - section.virtual_address as usize;
        let lut_table: &[u32] = unsafe { mem::transmute(&idata[lut_offset..]) };
        let thunk_table: &[u32] = unsafe { mem::transmute(&idata[thunk_offset..]) };
        let mut thunks = HashMap::new();
        let mut ordinal = 0usize;
        while lut_table[ordinal] != 0 {
            ensure!(lut_offset + mem::size_of::<u32>() * ordinal < section.virtual_size as usize, "lut past idata section");
            ensure!(lut_table[ordinal] == thunk_table[ordinal], "names and thunks must match");
            ensure!(lut_table[ordinal] >> 31 == 0, "only rva luts are supported");
            let name_table_rva = lut_table[ordinal] & 0x7FFF_FFFF;
            ensure!(name_table_rva > section.virtual_address, "import name table not in idata");
            ensure!(name_table_rva < section.virtual_address + section.virtual_size, "import name table not in idata");
            let name_table_offset = name_table_rva as usize - section.virtual_address as usize;
            let hint_ptr: *const u16 = idata[name_table_offset..].as_ptr() as *const _;
            let hint: u16 = unsafe { *hint_ptr };
            ensure!(hint == 0, "hint table entries are not supported");
            let name = Self::read_name(&idata[name_table_offset + 2..]).chain_err(|| "read name")?;
            let vaddr = dir.thunk_table as usize + ordinal * mem::size_of::<u32>();
            let thunk = Thunk {
                name,
                ordinal: ordinal as u32,
                vaddr: vaddr as u32,
            };
            thunks.insert(vaddr as u32, thunk);
            ordinal += 1;
        }
        return Ok(thunks);
    }

    fn read_name(n: &[u8]) -> Result<String> {
        let end_offset: usize = n.iter().position(|&c| c == 0).ok_or::<Error>(ErrorKind::NameError.into()).chain_err(|| "find end")?;
        return Ok(str::from_utf8(&n[..end_offset]).chain_err(|| "names should be utf8 encoded")?.to_owned());
    }

    fn _parse_relocs(relocs: &[u8], code_section: &SectionHeader) -> Result<Vec<u32>> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        let mut cnt = 0;
        while offset < relocs.len() {
            let base_reloc_ptr: *const BaseRelocation = relocs[offset..].as_ptr() as *const _;
            let base_reloc: &BaseRelocation = unsafe { &*base_reloc_ptr };
            cnt += 1;
            offset += 8 + base_reloc.page_rva as usize * 2;
            if base_reloc.block_size > 0 {
                let reloc_cnt = (base_reloc.block_size as usize - mem::size_of::<BaseRelocation>()) / 2;
                let relocs: &[u16] = unsafe { mem::transmute(&relocs[mem::size_of::<BaseRelocation>()..]) };
                for i in 0..reloc_cnt {
                    let flags = (relocs[i] & 0xF000) >> 12;
                    if flags == 0 {
                        continue;
                    }
                    let offset = relocs[i] & 0x0FFF;
                    ensure!(flags == 3, "only 32bit relocations are supported");
                    let rva = base_reloc.page_rva + offset as u32;
                    ensure!(rva >= code_section.virtual_address, "relocation not in CODE");
                    ensure!(rva < code_section.virtual_address + code_section.virtual_size, "relocation not in CODE");
                    let code_offset = (base_reloc.page_rva - code_section.virtual_address) + offset as u32;
                    out.push(code_offset);
                }
            }
        }
        return Ok(out);
    }
}


#[repr(C)]
#[repr(packed)]
struct COFFHeader {
    machine: u16,
    number_of_sections: u16,
    time_date_stamp: u32,
    pointer_to_symbol_table: u32,
    number_of_symbols: u32,
    size_of_optional_header: u16,
    characteristics: u16,
}

#[repr(C)]
#[repr(packed)]
struct OptionalHeader {
    magic: u16,
    major_linker_version: u8,
    minor_linker_version: u8,
    size_of_code: u32,
    size_of_initialized_data: u32,
    size_of_uninitialized_data: u32,
    address_of_entry_point: u32,
    base_of_code: u32,
    base_of_data: u32,
}

#[repr(C)]
#[repr(packed)]
struct WindowsHeader {
    image_base: u32,
    section_alignment: u32,
    file_alignment: u32,
    major_os_version: u16,
    minor_os_version: u16,
    major_image_version: u16,
    minor_image_version: u16,
    major_subsystem_version: u16,
    minor_subsystem_version: u16,
    win32_version_value: u32,
    size_of_image: u32,
    size_of_headers: u32,
    checksum: u32,
    subsystem: u16,
    dll_characteristics: u16,
    size_of_stack_reserve: u32,
    size_of_stack_commit: u32,
    size_of_heap_reserve: u32,
    size_of_heap_commit: u32,
    loader_flags: u32,
    number_of_rvas_and_sizes: u32
}

#[repr(C)]
#[repr(packed)]
struct DataDirectory {
    virtual_address: u32,
    size: u32
}

#[derive(Debug)]
#[repr(C)]
#[repr(packed)]
struct SectionHeader {
    name: [u8; 8],
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
    pointer_to_relocations: u32,
    pointer_to_line_numbers: u32,
    number_of_relocations: u16,
    number_of_line_numbers: u16,
    characteristics: u32
}

bitflags! {
    struct SectionFlags : u32 {
        const _1 = 0x00000001;  // Reserved for future use.
        const _2 = 0x00000002;  // Reserved for future use.
        const _3 = 0x00000004;  // Reserved for future use.
        const IMAGE_SCN_TYPE_NO_PAD = 0x00000008;  // The section should not be padded to the next boundary. This flag is obsolete and is replaced by IMAGE_SCN_ALIGN_1BYTES. This is valid only for object files.
        const _5 = 0x00000010;  // Reserved for future use.
        const IMAGE_SCN_CNT_CODE = 0x00000020;  // The section contains executable code.
        const IMAGE_SCN_CNT_INITIALIZED_DATA = 0x00000040;  // The section contains initialized data.
        const IMAGE_SCN_CNT_UNINITIALIZED_DATA = 0x00000080;  // The section contains uninitialized data.
        const IMAGE_SCN_LNK_OTHER = 0x00000100;  // Reserved for future use.
        const IMAGE_SCN_LNK_INFO = 0x00000200; // The section contains comments or other information. The .drectve section has this type. This is valid for object files only.
        const _B = 0x00000400;  // Reserved for future use.
        const IMAGE_SCN_LNK_REMOVE = 0x00000800;  // The section will not become part of the image. This is valid only for object files.
        const IMAGE_SCN_LNK_COMDAT = 0x00001000;  // The section contains COMDAT data. For more information, see COMDAT Sections (Object Only). This is valid only for object files.
        const IMAGE_SCN_GPREL = 0x00008000;  // The section contains data referenced through the global pointer (GP).
        const IMAGE_SCN_MEM_PURGEABLE = 0x00020000;  // Reserved for future use.
        const IMAGE_SCN_MEM_16BIT = 0x00020000;  // Reserved for future use.
        const IMAGE_SCN_MEM_LOCKED = 0x00040000;  // Reserved for future use.
        const IMAGE_SCN_MEM_PRELOAD = 0x00080000;  // Reserved for future use.
        const IMAGE_SCN_ALIGN_1BYTES = 0x00100000;  // Align data on a 1-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_2BYTES = 0x00200000;  // Align data on a 2-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_4BYTES = 0x00300000;  // Align data on a 4-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_8BYTES = 0x00400000;  // Align data on an 8-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_16BYTES = 0x00500000;  // Align data on a 16-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_32BYTES = 0x00600000;  // Align data on a 32-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_64BYTES = 0x00700000;  // Align data on a 64-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_128BYTES = 0x00800000;  // Align data on a 128-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_256BYTES = 0x00900000;  // Align data on a 256-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_512BYTES = 0x00A00000;  // Align data on a 512-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_1024BYTES = 0x00B00000;  // Align data on a 1024-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_2048BYTES = 0x00C00000;  // Align data on a 2048-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_4096BYTES = 0x00D00000;  // Align data on a 4096-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_8192BYTES = 0x00E00000;  // Align data on an 8192-byte boundary. Valid only for object files.
        const IMAGE_SCN_LNK_NRELOC_OVFL = 0x01000000;  // The section contains extended relocations.
        const IMAGE_SCN_MEM_DISCARDABLE = 0x02000000;  // The section can be discarded as needed.
        const IMAGE_SCN_MEM_NOT_CACHED = 0x04000000;  // The section cannot be cached.
        const IMAGE_SCN_MEM_NOT_PAGED = 0x08000000;  // The section is not pageable.
        const IMAGE_SCN_MEM_SHARED = 0x10000000;  // The section can be shared in memory.
        const IMAGE_SCN_MEM_EXECUTE = 0x20000000;  // The section can be executed as code.
        const IMAGE_SCN_MEM_READ = 0x40000000;  // The section can be read.
        const IMAGE_SCN_MEM_WRITE = 0x80000000;  // The section can be written to.
    }
}

impl SectionFlags {
    fn from_u32(u: u32) -> SectionFlags {
        unsafe { mem::transmute(u) }
    }
}

#[derive(Debug)]
#[repr(C)]
#[repr(packed)]
struct ImportDirectoryEntry {
    import_lut_rva: u32,
    timestamp: u32,
    forwarder_chain: u32,
    name_rva: u32,
    thunk_table: u32,
}

#[derive(Debug)]
#[repr(C)]
#[repr(packed)]
struct BaseRelocation {
    page_rva: u32,
    block_size: u32,
}

const DOSX_HEADER: &[u8] = &[68, 88, 0, 0, 0, 0, 1, 0, 16, 0, 6, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use super::*;

    #[test]
    fn it_works() {
        let paths = fs::read_dir("./test_data/").unwrap();
        for i in paths {
            let entry = i.unwrap();
            let path = format!("{}", entry.path().display());
            //println!("At: {}", path);

            let mut fp = fs::File::open(&path).unwrap();
            let mut data = Vec::new();
            fp.read_to_end(&mut data).unwrap();
            let pe = PE::parse(&data).unwrap();
        }
    }
}
