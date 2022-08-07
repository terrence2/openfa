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
use ansi::ansi;
use anyhow::{anyhow, bail, ensure, Result};
use bitflags::bitflags;
use log::trace;
use packed_struct::packed_struct;
use std::{collections::HashMap, mem, str};
use thiserror::Error;
use zerocopy::LayoutVerified;

#[derive(Debug, Error)]
enum PortableExecutableError {
    #[error("name ran off end of file")]
    NameUnending {},
}

pub struct PortableExecutable {
    // Maps from vaddr (as we may see in CODE) to the function name to thunk to.
    pub thunks: Vec<Thunk>,

    // Indirect jump block at end of code
    pub trampolines: Vec<Trampoline>,

    // A list of offsets in CODE containing a 32bit address that needs to be relocated in memory.
    // The base is always 0, so just add the address of CODE.
    pub relocs: Vec<u32>,

    // The code itself, copied out of the source.
    pub code: Vec<u8>,

    // Stored section headers, so that we can interpret thunks and relocs.
    pub section_info: HashMap<String, SectionInfo>,

    // Whatever value relocate was called with.
    pub relocation_target: u32,

    // The assumed mmap location of the file.
    pub image_base: u32,

    // The assumed load address of the code.
    pub code_vaddr: u32,

    // The actual load address of the code.
    pub code_addr: u32,
}

#[derive(Debug, Clone)]
pub struct Thunk {
    // The name of this import.
    pub name: String,

    // The ordinal of this import.
    pub ordinal: u32,

    // Virtual address of the thunk of this symbol.
    pub vaddr: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Trampoline {
    // Offset is from the start of the code section.
    pub offset: usize,

    // The name attached to the thunk that would populate this trampoline.
    pub name: String,

    // Where this trampoline would indirect to, if jumped to.
    pub target: u32,

    // Shape files call into engine functions by setting up a stack frame
    // and then returning. The target of this is always one of these trampolines
    // stored at the tail of the PE. Store the in-memory location of the
    // thunk for fast comparison with relocated addresses.
    pub mem_location: u32,

    // Whatever tool was used to link .SH's bakes in a direct pointer to the GOT
    // PLT base (e.g. target) as the data location. Presumably when doing
    // runtime linking, it uses the IAT's name as a tag and rewrites the direct
    // load to the real address of the symbol (and not a split read of the code
    // and reloc in the GOT). These appear to be both global and per-object data
    // depending on the data -- e.g. brentObjectId is probably per-object and
    // _currentTicks is probably global?
    //
    // A concrete example; if the inline assembly does:
    //    `mov %ebp, [<addr of GOT of data>]`
    //
    // The runtime would presumably use the relocation of the above addr as an
    // opportunity to rewrite the load as a reference to the real memory. We
    // need to take all of this into account when interpreting the referencing
    // code.
    pub is_data: bool,
}

impl Trampoline {
    pub const SIZE: usize = 6;

    pub fn has_trampoline(offset: usize, code: &[u8]) -> bool {
        // pe.section_info.contains_key(".idata") &&
        code.len() >= offset + Self::SIZE && code[offset] == 0xFF && code[offset + 1] == 0x25
    }

    pub fn from_code(
        thunks: &[Thunk],
        image_vbase: u32,
        offset: usize,
        code: &[u8],
    ) -> Result<Self> {
        ensure!(Self::has_trampoline(offset, code), "not a trampoline");
        let target = TrampolineEntry::overlay(&code[offset..offset + Self::SIZE])?.target;
        let thunk = Self::find_matching_thunk(image_vbase, target, thunks)?;
        // FIXME: update is_data in caller?
        // let is_data = DATA_RELOCATIONS.contains(&thunk.name);
        Ok(Trampoline {
            offset,
            name: thunk.name.clone(),
            target,
            mem_location: offset as u32,
            is_data: false,
        })
    }

    fn find_matching_thunk(image_vbase: u32, addr: u32, thunks: &[Thunk]) -> Result<&Thunk> {
        // The reason we have to process trampolines during load and not, perhaps,
        // after relocation is that not all SH files contain relocs for the thunk
        // targets(!). This is yet more evidence that they're not actually using
        // LoadLibrary to put shapes in memory. They're probably only using the
        // relocation list to rewrite data access with the thunks as tags. We're
        // using relocation, however, to help decode. So if the thunks are not
        // relocated automatically we have to check the relocated value
        // manually.

        // Since we're checking before relocation, this should "just work", even
        // though not all of the pointers here have relocation entries. e.g. the
        // raw values should be the same.
        trace!("looking for target 0x{:X} in {} thunks", addr, thunks.len());
        for thunk in thunks.iter() {
            if addr == thunk.vaddr {
                return Ok(thunk);
            }
        }

        // Also, in USNF, some of the thunks contain the base address already,
        // so treat them like a normal code pointer.
        let thunk_target = addr - image_vbase;
        trace!(
            "looking for target 0x{:X} in {} thunks",
            thunk_target,
            thunks.len()
        );
        for thunk in thunks.iter() {
            if thunk_target == thunk.vaddr {
                return Ok(thunk);
            }
        }

        bail!("did not find thunk with a target of {:08X}", thunk_target)
    }

    pub fn size(&self) -> usize {
        6
    }

    pub fn magic(&self) -> &'static str {
        "Tramp"
    }

    pub fn at_offset(&self) -> usize {
        self.offset
    }

    pub fn show(&self) -> String {
        format!(
            "@{:04X} {}Tramp{}: {}{}{} = {:04X}",
            self.offset,
            ansi().yellow().bold(),
            ansi(),
            ansi().yellow(),
            self.name,
            ansi(),
            self.target
        )
    }
}

pub struct SectionInfo {
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub size_of_raw_data: u32,
}

impl SectionInfo {
    fn from_header(header: &SectionHeader) -> Self {
        Self {
            virtual_address: header.virtual_address(),
            virtual_size: header.virtual_size(),
            size_of_raw_data: header.size_of_raw_data(),
        }
    }
}

impl PortableExecutable {
    pub fn from_bytes(data: &[u8]) -> Result<PortableExecutable> {
        assert_eq!(mem::size_of::<COFFHeader>(), 20);
        assert_eq!(mem::size_of::<OptionalHeader>(), 28);
        assert_eq!(mem::size_of::<WindowsHeader>(), 68);

        ensure!(data.len() > 0x3c + 4, "pe file too short for dos header");
        ensure!(
            data[0] == 0x4d && data[1] == 0x5a,
            "not a dos program file header"
        );
        let pe_offset = u32::from_le_bytes((&data[0x3c..0x40]).try_into()?) as usize;

        ensure!(
            data.len() > pe_offset + 4 + 20 + 28,
            "pe file to short for coff headers"
        );
        ensure!(
            data[pe_offset] as char == 'P' && data[pe_offset + 1] as char == 'L',
            "did not find pe header"
        );
        let coff_offset = pe_offset + 4;
        let coff = COFFHeader::overlay_prefix(&data[coff_offset..])?;
        ensure!(coff.machine() == 0x14C, "expected i386 machine type field");
        ensure!(
            coff.characteristics() == 0xA18E,
            "expected a specific set of coff flags"
        );
        ensure!(
            coff.pointer_to_symbol_table() == 0,
            "expected nil symbol table"
        );
        ensure!(coff.number_of_symbols() == 0, "expected no symbols");
        ensure!(coff.size_of_optional_header() == 224, "a normal PE file");

        let opt_offset = pe_offset + 4 + mem::size_of::<COFFHeader>();
        let opt = OptionalHeader::overlay_prefix(&data[opt_offset..])?;
        ensure!(opt.magic() == 0x10B, "expected a PE optional header magic");
        ensure!(
            opt.size_of_uninitialized_data() == 0,
            "expected no uninitialized data"
        );
        ensure!(
            opt.address_of_entry_point() == 0,
            "expected entry to be at zero"
        );
        ensure!(
            opt.base_of_code() == 0 || opt.base_of_code() == 4096,
            "expected code to live at page 0 or 1"
        );
        // opt.size_of_code
        // opt.size_of_initialize_data
        // opt.base_of_data

        let win_offset =
            pe_offset + 4 + mem::size_of::<COFFHeader>() + mem::size_of::<OptionalHeader>();
        let win = WindowsHeader::overlay_prefix(&data[win_offset..])?;
        ensure!(
            win.image_base() == 0 || win.image_base() == 0x10000,
            "expected image base to be 0 or 10000"
        );
        ensure!(
            win.section_alignment() == 4096,
            "expected page aligned memory sections"
        );
        ensure!(
            win.file_alignment() == 512,
            "expected block aligned file sections"
        );
        ensure!(
            win.major_image_version() == 0,
            "major image version should be unset"
        );
        ensure!(
            win.minor_image_version() == 0,
            "minor image version should be unset"
        );
        ensure!(
            win.win32_version_value() == 0,
            "win32 version version should be unset"
        );
        ensure!(
            win.size_of_headers() == 1024 || win.size_of_headers() == 512,
            "expected exactly 1K of headers"
        );
        ensure!(win.checksum() == 0, "checksum should be unset");
        ensure!(win.subsystem() == 66, "subsystem must be exactly 66");
        ensure!(
            win.dll_characteristics() == 0,
            "dll_characteristics should be unset"
        );
        ensure!(win.size_of_stack_reserve() == 0, "stack not supported");
        ensure!(win.size_of_stack_commit() == 0, "stack not supported");
        ensure!(win.loader_flags() == 0, "loader flags should not be set");
        // win.size_of_heap_reserve = 1M
        // win.size_of_heap_commit = 4096
        // win.number_of_rvas_and_sizes == 16
        // win.size_of_image

        // Load directory data so we can cross reference with the section labels.
        let dir_offset = win_offset + mem::size_of::<WindowsHeader>();
        let dir_end = dir_offset + 16 * mem::size_of::<DataDirectory>();
        let dirs = DataDirectory::overlay_slice(&data[dir_offset..dir_end])?;

        let section_table_offset =
            pe_offset + 4 + mem::size_of::<COFFHeader>() + coff.size_of_optional_header() as usize;
        let mut sections = HashMap::new();
        for i in 0..coff.number_of_sections() as usize {
            let section_offset = section_table_offset + i * mem::size_of::<SectionHeader>();
            let section = SectionHeader::overlay_prefix(&data[section_offset..])?;
            ensure!(
                section.number_of_relocations() == 0,
                "relocations are not supported"
            );
            ensure!(
                section.pointer_to_relocations() == 0,
                "relocations are not supported"
            );
            ensure!(
                section.number_of_line_numbers() == 0,
                "line numbers are not supported"
            );
            ensure!(
                section.pointer_to_line_numbers() == 0,
                "line numbers are not supported"
            );

            let name_raw = match section.name().iter().position(|&n| n == 0u8) {
                Some(last) => section.name()[..last].to_owned(),
                None => section.name()[0..8].to_owned(),
            };
            let name = str::from_utf8(&name_raw)?;

            let expect_flags = match name {
                "CODE" | ".text" => {
                    SectionFlags::IMAGE_SCN_CNT_CODE
                        | SectionFlags::IMAGE_SCN_MEM_EXECUTE
                        | SectionFlags::IMAGE_SCN_MEM_READ
                        | SectionFlags::IMAGE_SCN_MEM_WRITE
                }
                ".idata" => {
                    ensure!(
                        dirs[1].virtual_address() == section.virtual_address(),
                        "mismatched virtual address on .idata section"
                    );
                    SectionFlags::IMAGE_SCN_CNT_INITIALIZED_DATA
                        | SectionFlags::IMAGE_SCN_MEM_READ
                        | SectionFlags::IMAGE_SCN_MEM_WRITE
                }
                ".reloc" => {
                    ensure!(
                        dirs[5].virtual_address() == section.virtual_address(),
                        "mismatched virtual address on .reloc section"
                    );
                    SectionFlags::IMAGE_SCN_CNT_INITIALIZED_DATA
                        | SectionFlags::IMAGE_SCN_MEM_DISCARDABLE
                        | SectionFlags::IMAGE_SCN_MEM_READ
                }
                "$$DOSX" => {
                    SectionFlags::IMAGE_SCN_CNT_INITIALIZED_DATA
                        | SectionFlags::IMAGE_SCN_MEM_DISCARDABLE
                        | SectionFlags::IMAGE_SCN_MEM_READ
                }
                ".bss" => {
                    SectionFlags::IMAGE_SCN_CNT_UNINITIALIZED_DATA
                        | SectionFlags::IMAGE_SCN_MEM_READ
                        | SectionFlags::IMAGE_SCN_MEM_WRITE
                }
                s => bail!("unexpected section name: {}", s),
            };
            ensure!(
                SectionFlags::from_bits(section.characteristics())
                    .ok_or_else(|| anyhow!("invalid section flags in characteristics"))?
                    == expect_flags,
                "unexpected section flags"
            );

            trace!(
                "Section {} starting at offset {:X} loaded at vaddr {:X} base {:X}",
                name,
                section.pointer_to_raw_data(),
                section.virtual_address(),
                win.image_base(),
            );
            let start = section.pointer_to_raw_data() as usize;
            let end = start + section.virtual_size() as usize;
            let section_data = &data[start..end];
            if name == "$$DOSX" {
                ensure!(
                    section_data == DOSX_HEADER,
                    "expected a fixed-content DOSX header"
                );
                continue;
            }

            sections.insert(name.to_owned(), (section, section_data));
        }

        let mut thunks = Vec::new();
        if sections.contains_key(".idata") {
            let (idata_section, idata) = sections[".idata"];
            thunks.append(&mut PortableExecutable::parse_idata(idata_section, idata)?);
        }

        if !sections.contains_key("CODE") && !sections.contains_key(".text") {
            let (_, reloc_data) = sections[".reloc"];
            let relocs = PortableExecutable::parse_relocs(reloc_data, None)?;
            return Ok(PortableExecutable {
                thunks,
                trampolines: Vec::new(),
                relocs,
                code: Vec::new(),
                section_info: Self::owned_section_info(&sections),
                relocation_target: 0,
                image_base: win.image_base(),
                code_vaddr: 0,
                code_addr: 0,
            });
        }

        let (code_section, code) = if sections.contains_key("CODE") {
            sections["CODE"]
        } else {
            sections[".text"]
        };
        let (_, reloc_data) = sections[".reloc"];
        let relocs = PortableExecutable::parse_relocs(reloc_data, Some(code_section))?;
        let trampolines = PortableExecutable::parse_trampolines(
            &thunks,
            win.image_base(), /*+ code_section.virtual_address()*/
            code,
        )?;

        Ok(PortableExecutable {
            thunks,
            trampolines,
            relocs,
            code: code.to_owned(),
            section_info: Self::owned_section_info(&sections),
            relocation_target: 0,
            image_base: win.image_base(),
            code_vaddr: code_section.virtual_address(),
            code_addr: code_section.virtual_address(),
        })
    }

    fn owned_section_info(
        sections: &HashMap<String, (&SectionHeader, &[u8])>,
    ) -> HashMap<String, SectionInfo> {
        sections
            .iter()
            .map(|(name, (header, _))| ((*name).to_owned(), SectionInfo::from_header(header)))
            .collect::<HashMap<String, SectionInfo>>()
    }

    fn parse_idata(section: &SectionHeader, idata: &[u8]) -> Result<Vec<Thunk>> {
        ensure!(
            idata.len() > mem::size_of::<ImportDirectoryEntry>() * 2,
            "section data too short for directory"
        );

        // Assert that there is exactly one entry by loading the second section and checking
        // that it is null.
        let dirs_end = 2 * mem::size_of::<ImportDirectoryEntry>();
        let dirs = ImportDirectoryEntry::overlay_slice(&idata[..dirs_end])?;
        let dir = &dirs[0];
        let term = &dirs[1];
        ensure!(
            term.import_lut_rva() == 0
                && term.timestamp() == 0
                && term.forwarder_chain() == 0
                && term.name_rva() == 0
                && term.thunk_table() == 0,
            "expected one import dirctory entry"
        );
        ensure!(dir.timestamp() == 0, "expected zero timestamp");
        ensure!(dir.forwarder_chain() == 0, "did not expect forwarding");

        // Check that the name is main.dll.
        ensure!(
            dir.name_rva() > section.virtual_address(),
            "dll name not in section"
        );
        ensure!(
            dir.name_rva() < section.virtual_address() + section.virtual_size(),
            "dll name not in section"
        );
        let dll_name_offset = dir.name_rva() as usize - section.virtual_address() as usize;
        let dll_name = Self::read_name(&idata[dll_name_offset..])?;
        ensure!(
            dll_name == "main.dll",
            "expected the directory entry to be for main.dll"
        );

        // Iterate the name/thunk tables in parallel, extracting vaddr and name mappings.
        let lut_offset = dir.import_lut_rva() as usize - section.virtual_address() as usize;
        let max_luts = idata[lut_offset..].len() / mem::size_of::<u32>();
        let (lut_table, _) =
            LayoutVerified::<&[u8], [u32]>::new_slice_from_prefix(&idata[lut_offset..], max_luts)
                .ok_or_else(|| anyhow!("failed to transmute lut_table"))?;
        let thunk_offset = dir.thunk_table() as usize - section.virtual_address() as usize;
        let max_thunks = idata[thunk_offset..].len() / mem::size_of::<u32>();
        let (thunk_table, _) = LayoutVerified::<&[u8], [u32]>::new_slice_from_prefix(
            &idata[thunk_offset..],
            max_thunks,
        )
        .ok_or_else(|| anyhow!("failed to transmute thunk table"))?;
        let mut thunks = Vec::new();
        let mut ordinal = 0usize;
        while lut_table[ordinal] != 0 {
            ensure!(
                lut_offset + mem::size_of::<u32>() * ordinal < section.virtual_size() as usize,
                "lut past idata section"
            );
            ensure!(
                lut_table[ordinal] == thunk_table[ordinal],
                "names and thunks must match"
            );
            ensure!(lut_table[ordinal] >> 31 == 0, "only rva luts are supported");
            let name_table_rva = lut_table[ordinal] & 0x7FFF_FFFF;
            ensure!(
                name_table_rva > section.virtual_address(),
                "import name table not in idata"
            );
            ensure!(
                name_table_rva < section.virtual_address() + section.virtual_size(),
                "import name table not in idata"
            );
            let name_table_offset = name_table_rva as usize - section.virtual_address() as usize;
            let hint =
                u16::from_le_bytes((&idata[name_table_offset..name_table_offset + 2]).try_into()?);
            ensure!(hint == 0, "hint table entries are not supported");
            let name = Self::read_name(&idata[name_table_offset + 2..])?;
            let vaddr = dir.thunk_table() as usize + ordinal * mem::size_of::<u32>();
            let vaddr_offset = dir.thunk_table() as usize + ordinal * mem::size_of::<u32>();
            let vaddr_data_offset = vaddr_offset - section.virtual_address() as usize;
            let vaddr_data =
                u32::from_le_bytes((&idata[vaddr_data_offset..vaddr_data_offset + 4]).try_into()?);
            trace!(
                "Loaded thunk: {} for {} at {:04X} which contains: {:08X}",
                ordinal,
                name,
                vaddr,
                vaddr_data
            );
            let thunk = Thunk {
                name,
                ordinal: ordinal as u32,
                vaddr: vaddr as u32,
            };
            thunks.push(thunk);
            ordinal += 1;
        }
        Ok(thunks)
    }

    fn read_name(n: &[u8]) -> Result<String> {
        let end_offset: usize = n
            .iter()
            .position(|&c| c == 0)
            .ok_or::<PortableExecutableError>(PortableExecutableError::NameUnending {})?;
        Ok(str::from_utf8(&n[..end_offset])?.to_owned())
    }

    fn parse_relocs(relocs: &[u8], code_section: Option<&SectionHeader>) -> Result<Vec<u32>> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        trace!(
            "relocs section is 0x{:04X} bytes: {:?}",
            relocs.len(),
            &relocs[0..18]
        );
        while offset < relocs.len() {
            let base_reloc = BaseRelocation::overlay_prefix(&relocs[offset..])?;
            trace!("base reloc at {} is {:?}", offset, base_reloc);
            if base_reloc.block_size() > 0 {
                let reloc_cnt =
                    (base_reloc.block_size() as usize - mem::size_of::<BaseRelocation>()) / 2;
                let relocs_offset = offset + mem::size_of::<BaseRelocation>();
                let relocs = LayoutVerified::<&[u8], [u16]>::new_slice(&relocs[relocs_offset..])
                    .ok_or_else(|| anyhow!("failed to transmute relocations"))?;
                for reloc in relocs.iter().take(reloc_cnt) {
                    let flags = (reloc & 0xF000) >> 12;
                    if flags == 0 {
                        continue;
                    }
                    let reloc_offset = reloc & 0x0FFF;
                    ensure!(flags == 3, "only 32bit relocations are supported");
                    let rva = base_reloc.page_rva() + u32::from(reloc_offset);
                    ensure!(
                        rva >= Self::maybe_code_vaddr(code_section),
                        "relocation before CODE"
                    );
                    ensure!(
                        rva < Self::maybe_code_vaddr(code_section)
                            + Self::maybe_code_vsize(code_section),
                        "relocation after CODE"
                    );
                    let code_offset = (base_reloc.page_rva()
                        - Self::maybe_code_vaddr(code_section))
                        + u32::from(reloc_offset);
                    trace!(
                        "reloc at offset {} is {:04X} + {:04X} => rva:{:04X}, phys:{:04X}",
                        offset,
                        base_reloc.page_rva(),
                        reloc_offset,
                        rva,
                        code_offset
                    );
                    out.push(code_offset);
                }
            }
            offset += base_reloc.block_size() as usize;
            if base_reloc.block_size() == 0 {
                break;
            }
        }
        Ok(out)
    }

    fn maybe_code_vaddr(code_section: Option<&SectionHeader>) -> u32 {
        if let Some(cs) = code_section {
            return cs.virtual_address();
        }
        0
    }

    fn maybe_code_vsize(code_section: Option<&SectionHeader>) -> u32 {
        if let Some(cs) = code_section {
            return cs.virtual_size();
        }
        0
    }

    fn parse_trampolines(
        thunks: &[Thunk],
        image_vbase: u32,
        code: &[u8],
    ) -> Result<Vec<Trampoline>> {
        let mut trampolines = Vec::new();
        if code.len() < Trampoline::SIZE {
            return Ok(trampolines);
        }
        let mut offset = code.len() - Trampoline::SIZE;
        while offset > 0 {
            if Trampoline::has_trampoline(offset, code) {
                let tramp = Trampoline::from_code(thunks, image_vbase, offset, code)?;
                trace!("found trampoline: {}", tramp.show());
                trampolines.push(tramp);
            } else {
                break;
            }
            offset -= Trampoline::SIZE;
        }
        trampolines.reverse();
        Ok(trampolines)
    }

    pub fn relocate(&mut self, target: u32) -> Result<()> {
        self.relocation_target = target;
        let reloc_delta = RelocationDelta::new(target, self.image_base + self.code_vaddr);
        for &reloc in self.relocs.iter() {
            let reloc = reloc as usize;
            let pcode = u32::from_le_bytes((&self.code[reloc..reloc + 4]).try_into()?);
            trace!(
                "Relocating word at 0x{:04X} from 0x{:08X} to 0x{:08X}",
                reloc as usize,
                pcode,
                reloc_delta.apply(pcode)
            );
            // The safe version is less clear, but supports non little-endian architectures.
            // { *pcode = reloc_delta.apply(*pcode); }
            self.code[reloc..reloc + 4].copy_from_slice(&reloc_delta.apply(pcode).to_le_bytes());
        }

        // Note: section headers and thunks do not get image base I guess?
        let thunk_delta = RelocationDelta::new(target, self.code_vaddr);
        for info in self.section_info.values_mut() {
            trace!(
                "Relocating section vaddr: 0x{:08X} + 0x{:08X} = 0x{:08X}",
                info.virtual_address,
                thunk_delta.delta(),
                thunk_delta.apply(info.virtual_address)
            );
            info.virtual_address = thunk_delta.apply(info.virtual_address);
        }
        for thunk in self.thunks.iter_mut() {
            trace!(
                "Relocating thunk vaddr: 0x{:08X} + 0x{:08X} = 0x{:08X}",
                thunk.vaddr,
                thunk_delta.delta(),
                thunk_delta.apply(thunk.vaddr)
            );
            thunk.vaddr = thunk_delta.apply(thunk.vaddr);
        }

        // Note: trampolines are part of the code, so they already have vcode from reloc
        //       ...except for USNF, which doesn't include the trampoline table in relocs
        let tramp_delta = RelocationDelta::new(target, self.image_base);
        for trampoline in self.trampolines.iter_mut() {
            trace!(
                "Relocating trampoline location: 0x{:08X} + 0x{:08X} = 0x{:08X}",
                trampoline.mem_location,
                tramp_delta.delta(),
                tramp_delta.apply(trampoline.mem_location)
            );
            let mut mem_location = tramp_delta.apply(trampoline.mem_location);
            if mem_location & target != target {
                let usnf_delta = RelocationDelta::new(target, self.code_vaddr);
                trace!(
                    "Adjust failed trampoline with vaddr: {:08X} + {:08X} => {:08X}",
                    trampoline.mem_location,
                    usnf_delta.delta(),
                    usnf_delta.apply(trampoline.mem_location)
                );
                mem_location = usnf_delta.apply(trampoline.mem_location) + self.code_vaddr;
            }
            trampoline.mem_location = mem_location;
            assert_eq!(
                trampoline.mem_location & target,
                target,
                "failed relocation"
            );
        }

        Ok(())
    }
}

enum RelocationDelta {
    Up(u32),
    Down(u32),
}

impl RelocationDelta {
    pub fn new(target: u32, base: u32) -> Self {
        if target >= base {
            RelocationDelta::Up(target - base)
        } else {
            RelocationDelta::Down(base - target)
        }
    }

    pub fn apply(&self, vaddr: u32) -> u32 {
        match self {
            RelocationDelta::Up(delta) => vaddr + *delta,
            RelocationDelta::Down(delta) => vaddr - *delta,
        }
    }

    pub fn delta(&self) -> u32 {
        match self {
            RelocationDelta::Up(delta) => *delta,
            RelocationDelta::Down(delta) => *delta,
        }
    }
}

#[packed_struct]
struct COFFHeader {
    machine: u16,
    number_of_sections: u16,
    time_date_stamp: u32,
    pointer_to_symbol_table: u32,
    number_of_symbols: u32,
    size_of_optional_header: u16,
    characteristics: u16,
}

#[packed_struct]
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

#[packed_struct]
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
    number_of_rvas_and_sizes: u32,
}

#[packed_struct]
struct DataDirectory {
    virtual_address: u32,
    size: u32,
}

#[packed_struct]
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
    characteristics: u32,
}

#[packed_struct]
struct TrampolineEntry {
    jmp: u16,
    target: u32,
}

bitflags! {
    struct SectionFlags : u32 {
        const _1 = 0x0000_0001;  // Reserved for future use.
        const _2 = 0x0000_0002;  // Reserved for future use.
        const _3 = 0x0000_0004;  // Reserved for future use.
        const IMAGE_SCN_TYPE_NO_PAD = 0x0000_0008;  // The section should not be padded to the next boundary. This flag is obsolete and is replaced by IMAGE_SCN_ALIGN_1BYTES. This is valid only for object files.
        const _5 = 0x0000_0010;  // Reserved for future use.
        const IMAGE_SCN_CNT_CODE = 0x0000_0020;  // The section contains executable code.
        const IMAGE_SCN_CNT_INITIALIZED_DATA = 0x0000_0040;  // The section contains initialized data.
        const IMAGE_SCN_CNT_UNINITIALIZED_DATA = 0x0000_0080;  // The section contains uninitialized data.
        const IMAGE_SCN_LNK_OTHER = 0x0000_0100;  // Reserved for future use.
        const IMAGE_SCN_LNK_INFO = 0x0000_0200; // The section contains comments or other information. The .drectve section has this type. This is valid for object files only.
        const _B = 0x0000_0400;  // Reserved for future use.
        const IMAGE_SCN_LNK_REMOVE = 0x0000_0800;  // The section will not become part of the image. This is valid only for object files.
        const IMAGE_SCN_LNK_COMDAT = 0x0000_1000;  // The section contains COMDAT data. For more information, see COMDAT Sections (Object Only). This is valid only for object files.
        const IMAGE_SCN_GPREL = 0x0000_8000;  // The section contains data referenced through the global pointer (GP).
        const IMAGE_SCN_MEM_PURGEABLE = 0x0002_0000;  // Reserved for future use.
        const IMAGE_SCN_MEM_16BIT = 0x0002_0000;  // Reserved for future use.
        const IMAGE_SCN_MEM_LOCKED = 0x0004_0000;  // Reserved for future use.
        const IMAGE_SCN_MEM_PRELOAD = 0x0008_0000;  // Reserved for future use.
        const IMAGE_SCN_ALIGN_1BYTES = 0x0010_0000;  // Align data on a 1-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_2BYTES = 0x0020_0000;  // Align data on a 2-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_4BYTES = 0x0030_0000;  // Align data on a 4-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_8BYTES = 0x0040_0000;  // Align data on an 8-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_16BYTES = 0x0050_0000;  // Align data on a 16-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_32BYTES = 0x0060_0000;  // Align data on a 32-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_64BYTES = 0x0070_0000;  // Align data on a 64-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_128BYTES = 0x0080_0000;  // Align data on a 128-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_256BYTES = 0x0090_0000;  // Align data on a 256-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_512BYTES = 0x00A0_0000;  // Align data on a 512-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_1024BYTES = 0x00B0_0000;  // Align data on a 1024-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_2048BYTES = 0x00C0_0000;  // Align data on a 2048-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_4096BYTES = 0x00D0_0000;  // Align data on a 4096-byte boundary. Valid only for object files.
        const IMAGE_SCN_ALIGN_8192BYTES = 0x00E0_0000;  // Align data on an 8192-byte boundary. Valid only for object files.
        const IMAGE_SCN_LNK_NRELOC_OVFL = 0x0100_0000;  // The section contains extended relocations.
        const IMAGE_SCN_MEM_DISCARDABLE = 0x0200_0000;  // The section can be discarded as needed.
        const IMAGE_SCN_MEM_NOT_CACHED = 0x0400_0000;  // The section cannot be cached.
        const IMAGE_SCN_MEM_NOT_PAGED = 0x0800_0000;  // The section is not pageable.
        const IMAGE_SCN_MEM_SHARED = 0x1000_0000;  // The section can be shared in memory.
        const IMAGE_SCN_MEM_EXECUTE = 0x2000_0000;  // The section can be executed as code.
        const IMAGE_SCN_MEM_READ = 0x4000_0000;  // The section can be read.
        const IMAGE_SCN_MEM_WRITE = 0x8000_0000;  // The section can be written to.
    }
}

#[packed_struct]
struct ImportDirectoryEntry {
    import_lut_rva: u32,
    timestamp: u32,
    forwarder_chain: u32,
    name_rva: u32,
    thunk_table: u32,
}

#[packed_struct]
#[derive(Copy, Clone, Debug)]
struct BaseRelocation {
    page_rva: u32,
    block_size: u32,
}

const DOSX_HEADER: &[u8] = &[
    68, 88, 0, 0, 0, 0, 1, 0, 16, 0, 6, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0,
];

#[cfg(test)]
mod tests {
    use super::*;
    use lib::Libs;

    #[test]
    fn it_works() -> Result<()> {
        let libs = Libs::for_testing()?;
        for (game, _palette, catalog) in libs.selected() {
            for fid in catalog
                .find_with_extension("SH")?
                .iter()
                .chain(catalog.find_with_extension("LAY")?.iter())
                .chain(catalog.find_with_extension("DLG")?.iter())
                .chain(catalog.find_with_extension("MNU")?.iter())
                .chain(catalog.find_with_extension("MC")?.iter())
            {
                let meta = catalog.stat(*fid)?;
                println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                let data = catalog.read(*fid)?;
                let _pe = PortableExecutable::from_bytes(data.as_ref())?;
            }
        }

        Ok(())
    }
}
