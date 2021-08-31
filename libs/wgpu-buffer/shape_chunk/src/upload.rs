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
use crate::{
    draw_state::DrawState,
    texture_atlas::{Frame, MegaAtlas},
};
use absolute_unit::{feet, Feet, Length};
use anyhow::{anyhow, bail, ensure, Result};
use bitflags::bitflags;
use catalog::Catalog;
use geometry::Aabb;
use i386::Interpreter;
use lazy_static::lazy_static;
use log::trace;
use memoffset::offset_of;
use pal::Palette;
use parking_lot::RwLock;
use pic::Pic;
use sh::{Facet, FacetFlags, Instr, RawShape, VertexBuf, X86Code, X86Trampoline, SHAPE_LOAD_BASE};
use std::{
    collections::{HashMap, HashSet},
    mem,
    sync::Arc,
    time::Instant,
};
use zerocopy::{AsBytes, FromBytes};

const MAX_XFORM_ID: u32 = 32;

bitflags! {
    pub struct VertexFlags: u64 {
        const NONE                 = 0x0000_0000_0000_0000;

        // If set, manually blend the face color with the texture color.
        const BLEND_TEXTURE        = 0x0000_0000_0000_0001;

        // For each frame, the expected configuration of what we expect to be
        // drawn is set to true is set in PushConstants. Each vertex is tagged
        // with the part it is part of based on the reference in a controlling
        // script, if present.
        const STATIC               = 0x0000_0000_0000_0002;
        const AFTERBURNER_ON       = 0x0000_0000_0000_0004;
        const AFTERBURNER_OFF      = 0x0000_0000_0000_0008;
        const RIGHT_FLAP_DOWN      = 0x0000_0000_0000_0010;
        const RIGHT_FLAP_UP        = 0x0000_0000_0000_0020;
        const LEFT_FLAP_DOWN       = 0x0000_0000_0000_0040;
        const LEFT_FLAP_UP         = 0x0000_0000_0000_0080;
        const HOOK_EXTENDED        = 0x0000_0000_0000_0100;
        const HOOK_RETRACTED       = 0x0000_0000_0000_0200;
        const GEAR_UP              = 0x0000_0000_0000_0400;
        const GEAR_DOWN            = 0x0000_0000_0000_0800;
        const BRAKE_EXTENDED       = 0x0000_0000_0000_1000;
        const BRAKE_RETRACTED      = 0x0000_0000_0000_2000;
        const BAY_CLOSED           = 0x0000_0000_0000_4000;
        const BAY_OPEN             = 0x0000_0000_0000_8000;
        const RUDDER_CENTER        = 0x0000_0000_0001_0000;
        const RUDDER_LEFT          = 0x0000_0000_0002_0000;
        const RUDDER_RIGHT         = 0x0000_0000_0004_0000;
        const LEFT_AILERON_CENTER  = 0x0000_0000_0008_0000;
        const LEFT_AILERON_UP      = 0x0000_0000_0010_0000;
        const LEFT_AILERON_DOWN    = 0x0000_0000_0020_0000;
        const RIGHT_AILERON_CENTER = 0x0000_0000_0040_0000;
        const RIGHT_AILERON_UP     = 0x0000_0000_0080_0000;
        const RIGHT_AILERON_DOWN   = 0x0000_0000_0100_0000;
        const SLATS_DOWN           = 0x0000_0000_0200_0000;
        const SLATS_UP             = 0x0000_0000_0400_0000;

        const PLAYER_ALIVE         = 0x0000_0000_2000_0000;
        const PLAYER_DEAD          = 0x0000_0000_4000_0000;

        const ANIM_FRAME_0_2       = 0x0000_0001_0000_0000;
        const ANIM_FRAME_1_2       = 0x0000_0002_0000_0000;

        const ANIM_FRAME_0_3       = 0x0000_0004_0000_0000;
        const ANIM_FRAME_1_3       = 0x0000_0008_0000_0000;
        const ANIM_FRAME_2_3       = 0x0000_0010_0000_0000;

        const ANIM_FRAME_0_4       = 0x0000_0020_0000_0000;
        const ANIM_FRAME_1_4       = 0x0000_0040_0000_0000;
        const ANIM_FRAME_2_4       = 0x0000_0080_0000_0000;
        const ANIM_FRAME_3_4       = 0x0000_0100_0000_0000;

        const ANIM_FRAME_0_6       = 0x0000_0200_0000_0000;
        const ANIM_FRAME_1_6       = 0x0000_0400_0000_0000;
        const ANIM_FRAME_2_6       = 0x0000_0800_0000_0000;
        const ANIM_FRAME_3_6       = 0x0000_1000_0000_0000;
        const ANIM_FRAME_4_6       = 0x0000_2000_0000_0000;
        const ANIM_FRAME_5_6       = 0x0000_4000_0000_0000;

        const SAM_COUNT_0          = 0x0000_8000_0000_0000;
        const SAM_COUNT_1          = 0x0001_0000_0000_0000;
        const SAM_COUNT_2          = 0x0002_0000_0000_0000;
        const SAM_COUNT_3          = 0x0004_0000_0000_0000;

        const EJECT_STATE_0        = 0x0008_0000_0000_0000;
        const EJECT_STATE_1        = 0x0010_0000_0000_0000;
        const EJECT_STATE_2        = 0x0020_0000_0000_0000;
        const EJECT_STATE_3        = 0x0040_0000_0000_0000;
        const EJECT_STATE_4        = 0x0080_0000_0000_0000;

        const AILERONS_DOWN        = Self::LEFT_AILERON_DOWN.bits | Self::RIGHT_AILERON_DOWN.bits;
        const AILERONS_UP          = Self::LEFT_AILERON_UP.bits | Self::RIGHT_AILERON_UP.bits;
    }
}

impl VertexFlags {
    pub fn displacement(self, offset: usize) -> Result<Self> {
        VertexFlags::from_bits(self.bits() << offset).ok_or_else(|| {
            anyhow!(format!(
                "offset {} from {:?} did not yield a valid vertex flags",
                offset, self
            ))
        })
    }
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
pub struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    tex_coord: [f32; 2],
    flags0: u32,
    flags1: u32,
    xform_id: u32,
}

impl Vertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        let tmp = wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float3,
                    offset: 0,
                    shader_location: 0,
                },
                // color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float4,
                    offset: 12,
                    shader_location: 1,
                },
                // tex_coord
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float2,
                    offset: 28,
                    shader_location: 2,
                },
                // flags0
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint,
                    offset: 36,
                    shader_location: 3,
                },
                // flags1
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint,
                    offset: 40,
                    shader_location: 4,
                },
                // xform_id
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint,
                    offset: 44,
                    shader_location: 5,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(Vertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(Vertex, color) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(Vertex, tex_coord) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[3].offset,
            offset_of!(Vertex, flags0) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[4].offset,
            offset_of!(Vertex, flags1) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[5].offset,
            offset_of!(Vertex, xform_id) as wgpu::BufferAddress
        );

        tmp
    }
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            position: [0f32, 0f32, 0f32],
            color: [0.75f32, 0.5f32, 0f32, 1f32],
            tex_coord: [0f32, 0f32],
            flags0: 0,
            flags1: 0,
            xform_id: 0,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum DrawSelection {
    DamageModel,
    NormalModel,
}

impl DrawSelection {
    pub fn is_damage(&self) -> bool {
        self == &DrawSelection::DamageModel
    }
}

#[derive(Clone, Debug)]
pub enum TransformInput {
    CurrentTicks(u32),
    GearPosition(u32),
    GearDown(u32),
    BayPosition(u32),
    BayOpen(u32),
    CanardPosition(u32),
    AfterBurner(u32),
    VerticalOn(u32),
    VerticalAngle(u32),
    SwingWing(u32),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ShapeErrata {
    pub no_upper_aileron: bool,
    pub has_frame_animation: bool,
    pub has_xform_animation: bool,
    pub num_xform_animations: u8,
}

impl ShapeErrata {
    fn from_flags(analysis: &AnalysisResults) -> Self {
        let flags = analysis.prop_man.seen_flags;
        Self {
            // ERRATA: ATFNATO:F22.SH is missing aileron up meshes.
            no_upper_aileron: !(flags & VertexFlags::AILERONS_DOWN).is_empty()
                && (flags & VertexFlags::AILERONS_UP).is_empty(),
            has_frame_animation: analysis.has_frame_animation,
            has_xform_animation: !analysis.transformers.is_empty(),
            num_xform_animations: analysis.transformers.len() as u8,
        }
    }
}

// TODO: this should be a sibling of ShapeUploader in such a way that they can share
// TODO: the core iterate-instructions loop, but keep separate state around that.
pub(crate) struct AnalysisResults {
    has_frame_animation: bool,
    prop_man: BufferPropsManager,
    transformers: Vec<Transformer>,
}

impl Default for AnalysisResults {
    fn default() -> Self {
        Self {
            has_frame_animation: false,
            prop_man: BufferPropsManager::new(),
            transformers: Vec::new(),
        }
    }
}

impl AnalysisResults {
    pub fn has_animation(&self) -> bool {
        self.has_frame_animation
    }

    pub fn has_xforms(&self) -> bool {
        self.prop_man.next_xform_id == 0
    }

    pub fn has_flags(&self) -> bool {
        self.prop_man.seen_flags == VertexFlags::NONE
    }
}

lazy_static! {
    static ref TOGGLE_TABLE: HashMap<&'static str, Vec<(u32, VertexFlags)>> = {
        let mut table = HashMap::new();
        table.insert(
            "_PLrightFlap",
            vec![
                (0xFFFF_FFFF, VertexFlags::RIGHT_FLAP_DOWN),
                (0,           VertexFlags::RIGHT_FLAP_UP),
            ],
        );
        table.insert(
            "_PLleftFlap",
            vec![
                (0xFFFF_FFFF, VertexFlags::LEFT_FLAP_DOWN),
                (0,           VertexFlags::LEFT_FLAP_UP),
            ],
        );
        table.insert(
            "_PLslats",
            vec![(0, VertexFlags::SLATS_UP), (1, VertexFlags::SLATS_DOWN)],
        );
        table.insert(
            "_PLgearDown",
            vec![(0, VertexFlags::GEAR_UP), (1, VertexFlags::GEAR_DOWN)],
        );
        table.insert(
            "_PLbayOpen",
            vec![
                (0, VertexFlags::BAY_CLOSED),
                (1, VertexFlags::BAY_OPEN),
            ],
        );
        table.insert(
            "_PLbrake",
            vec![
                (0, VertexFlags::BRAKE_RETRACTED),
                (1, VertexFlags::BRAKE_EXTENDED),
            ],
        );
        table.insert(
            "_PLhook",
            vec![
                (0, VertexFlags::HOOK_RETRACTED),
                (1, VertexFlags::HOOK_EXTENDED),
            ],
        );
        table.insert(
            "_PLrudder",
            vec![
                // FIXME: this doesn't line up with our left/right above?
                (0, VertexFlags::RUDDER_CENTER),
                (1, VertexFlags::RUDDER_RIGHT),
                (0xFFFF_FFFF, VertexFlags::RUDDER_LEFT),
            ],
        );
        table.insert(
            "_PLrightAln",
            vec![
                (0, VertexFlags::RIGHT_AILERON_CENTER),
                (1, VertexFlags::RIGHT_AILERON_UP),
                (0xFFFF_FFFF, VertexFlags::RIGHT_AILERON_DOWN),
            ],
        );
        table.insert(
            "_PLleftAln",
            vec![
                (0, VertexFlags::LEFT_AILERON_CENTER),
                (1, VertexFlags::LEFT_AILERON_UP),
                (0xFFFF_FFFF, VertexFlags::LEFT_AILERON_DOWN),
            ],
        );
        table.insert(
            "_PLafterBurner",
            vec![
                (0, VertexFlags::AFTERBURNER_OFF),
                (1, VertexFlags::AFTERBURNER_ON),
            ],
        );
        table.insert(
            "_SAMcount",
            vec![
                (0, VertexFlags::SAM_COUNT_0),
                (1, VertexFlags::SAM_COUNT_1),
                (2, VertexFlags::SAM_COUNT_2),
                (3, VertexFlags::SAM_COUNT_3),
            ],
        );
        table.insert(
            "_PLstate",
            vec![
                (0x11, VertexFlags::EJECT_STATE_0),
                (0x12, VertexFlags::EJECT_STATE_1),
                (0x13, VertexFlags::EJECT_STATE_2),
                (0x14, VertexFlags::EJECT_STATE_3),
                (0x15, VertexFlags::EJECT_STATE_4),

                (0x1A, VertexFlags::EJECT_STATE_0),
                (0x1B, VertexFlags::EJECT_STATE_1),
                (0x1C, VertexFlags::EJECT_STATE_2),
                (0x1D, VertexFlags::EJECT_STATE_3),
                (0x1E, VertexFlags::EJECT_STATE_4),

                (0x22, VertexFlags::EJECT_STATE_0),
                (0x23, VertexFlags::EJECT_STATE_1),
                (0x24, VertexFlags::EJECT_STATE_2),
                (0x25, VertexFlags::EJECT_STATE_3),
                (0x26, VertexFlags::EJECT_STATE_4),
            ],
        );
        table.insert(
            "_PLdead",
            vec![
                (0, VertexFlags::PLAYER_ALIVE),
                (1, VertexFlags::PLAYER_DEAD),
            ],
        );
        table
    };

    static ref SKIP_TABLE: HashSet<&'static str> = {
        let mut table = HashSet::new();
        table.insert("lighteningAllowed");
        table
    };
}

struct ProgramCounter {
    instr_offset: usize,
    byte_offset: usize,

    // The number of instructions
    instr_limit: usize,
}

impl ProgramCounter {
    fn new(instr_limit: usize) -> Self {
        Self {
            instr_limit,
            instr_offset: 0,
            byte_offset: 0,
        }
    }

    // Return true if the pc is in bounds.
    fn valid(&self) -> bool {
        self.instr_offset < self.instr_limit
    }

    // Return true if the pc is current at the given byte offset
    fn matches_byte(&self, byte_offset: usize) -> bool {
        self.byte_offset == byte_offset
    }

    // Move the pc to the given byte offset
    fn set_byte_offset(&mut self, next_offset: usize, sh: &RawShape) -> Result<()> {
        self.byte_offset = next_offset;
        self.instr_offset = sh.bytes_to_index(next_offset)?;
        ensure!(self.valid(), "pc jumped out of bounds");
        Ok(())
    }

    // Get the current instructions.
    fn current_instr<'b>(&self, sh: &'b RawShape) -> &'b Instr {
        &sh.instrs[self.instr_offset]
    }

    fn relative_instr<'a>(&self, offset: isize, sh: &'a RawShape) -> &'a Instr {
        &sh.instrs[self.instr_offset.wrapping_add(offset as usize)]
    }

    fn advance(&mut self, sh: &RawShape) {
        self.byte_offset += self.current_instr(sh).size();
        self.instr_offset += 1;
    }
}

#[derive(Clone)]
struct BufferProps {
    context: String,
    flags: VertexFlags,
    xform_id: u32,
}

struct BufferPropsManager {
    props: HashMap<usize, BufferProps>,
    seen_flags: VertexFlags,
    next_xform_id: u32,
    active_xform_id: u32,
}

impl BufferPropsManager {
    pub fn new() -> Self {
        Self {
            seen_flags: VertexFlags::NONE,
            props: HashMap::new(),
            next_xform_id: 0,
            active_xform_id: 0,
        }
    }

    pub fn add_or_update_toggle_flags(&mut self, tgt: usize, flags: VertexFlags, context: &str) {
        let entry = self.props.entry(tgt).or_insert(BufferProps {
            context: context.to_owned(),
            flags: VertexFlags::NONE,
            xform_id: self.active_xform_id,
        });
        entry.flags |= flags;
        self.seen_flags |= flags;
    }

    pub fn add_xform_and_flags(
        &mut self,
        tgt: usize,
        flags: VertexFlags,
        context: &str,
    ) -> Result<u32> {
        ensure!(
            !self.props.contains_key(&tgt),
            "have already transformed that buffer"
        );

        let xform_id = self.next_xform_id;
        self.props.insert(
            tgt,
            BufferProps {
                context: context.to_owned(),
                flags,
                xform_id,
            },
        );
        self.next_xform_id += 1;

        Ok(xform_id)
    }
}

// More than meets the eye.
// Contains everything needed to update one of the sub-component transforms:
// the virtual machine interpreter, already set up, the xform_id it maps to
// for upload, the code and data offsets, and all inputs that need to be
// configured and where to put them.
#[derive(Clone, Debug)]
pub struct Transformer {
    xform_id: u32,
    vm: Interpreter,
    code_offset: u32,
    data_offset: u32,
    inputs: Vec<TransformInput>,
    xform_base: [u8; 12],
}

impl Transformer {
    pub fn offset(&self) -> usize {
        self.xform_id as usize
    }

    pub fn transform(
        &mut self,
        draw_state: &DrawState,
        start: &Instant,
        now: &Instant,
    ) -> Result<[f32; 6]> {
        fn fa2r(d: f32) -> f32 {
            d * std::f32::consts::PI / 8192f32
        }

        let vm = &mut self.vm;
        for input in &self.inputs {
            let (loc, value) = match input {
                TransformInput::CurrentTicks(loc) => {
                    (*loc, (((*now - *start).as_millis() as u32) >> 4) & 0x0FFF)
                }
                TransformInput::GearPosition(loc) => (*loc, draw_state.x86_gear_position()),
                TransformInput::GearDown(loc) => (*loc, draw_state.x86_gear_down()),
                TransformInput::BayPosition(loc) => (*loc, draw_state.x86_bay_position()),
                TransformInput::BayOpen(loc) => (*loc, draw_state.x86_bay_open()),
                TransformInput::CanardPosition(loc) => (*loc, draw_state.x86_canard_position()),
                TransformInput::AfterBurner(loc) => (*loc, draw_state.x86_afterburner_enabled()),
                TransformInput::VerticalAngle(loc) => (*loc, draw_state.x86_vertical_angle()),
                TransformInput::SwingWing(loc) => (*loc, draw_state.x86_swing_wing()),
                TransformInput::VerticalOn(loc) => {
                    (*loc, 0) // FIXME: need to figure out how harrier is going to work still
                }
            };
            vm.map_value(loc, value);
        }
        vm.map_writable(self.data_offset, self.xform_base.to_vec())?;
        let result = vm.interpret(self.code_offset)?;
        let (tramp, _) = result.ok_trampoline()?;
        ensure!(tramp == "do_start_interp", "unexpected interp result");
        let xformed = vm.unmap_writable(self.data_offset)?;
        #[allow(clippy::transmute_ptr_to_ptr)]
        let words: &[i16] = unsafe { std::mem::transmute(xformed.as_slice()) };
        let arr = [
            f32::from(words[0]),
            -f32::from(words[1]),
            -f32::from(words[2]),
            -fa2r(f32::from(words[4])),
            -fa2r(f32::from(words[3])),
            -fa2r(f32::from(words[5])),
        ];
        for input in &self.inputs {
            match input {
                TransformInput::CurrentTicks(loc) => vm.unmap_value(*loc),
                TransformInput::GearPosition(loc) => vm.unmap_value(*loc),
                TransformInput::GearDown(loc) => vm.unmap_value(*loc),
                TransformInput::BayPosition(loc) => vm.unmap_value(*loc),
                TransformInput::BayOpen(loc) => vm.unmap_value(*loc),
                TransformInput::CanardPosition(loc) => vm.unmap_value(*loc),
                TransformInput::AfterBurner(loc) => vm.unmap_value(*loc),
                TransformInput::VerticalOn(loc) => vm.unmap_value(*loc),
                TransformInput::VerticalAngle(loc) => vm.unmap_value(*loc),
                TransformInput::SwingWing(loc) => vm.unmap_value(*loc),
            };
        }
        Ok(arr)
    }
}

// Contains information about what parts of the shape can be mutated by
// standard actions. e.g. Gears, flaps, etc.
#[derive(Clone, Debug)]
pub struct ShapeWidgets {
    shape_name: String,

    // Self contained vm/instructions for how to set up each required transform
    // to draw this shape buffer.
    transformers: Vec<Transformer>,

    // Draw properties based on what's in the shape file.
    errata: ShapeErrata,

    // Bounding box
    aabb: Aabb<f32, 3>,
}

impl ShapeWidgets {
    pub fn new(
        name: &str,
        errata: ShapeErrata,
        transformers: Vec<Transformer>,
        aabb: Aabb<f32, 3>,
    ) -> Self {
        Self {
            shape_name: name.to_owned(),
            transformers,
            errata,
            aabb,
        }
    }

    pub fn animate_into(
        &mut self,
        draw_state: &DrawState,
        start: &Instant,
        now: &Instant,
        buffer: &mut [[f32; 6]],
    ) -> Result<()> {
        assert!(buffer.len() >= self.num_xforms());
        for (offset, transformer) in self.transformers.iter_mut().enumerate() {
            let xform = transformer.transform(draw_state, start, now)?;
            buffer[offset].copy_from_slice(&xform);
        }
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.shape_name
    }

    pub fn errata(&self) -> ShapeErrata {
        self.errata
    }

    pub fn num_xforms(&self) -> usize {
        self.transformers.len()
    }

    pub fn num_transformer_floats(&self) -> usize {
        self.transformers.len() * 6
    }

    // In shape units.
    pub fn height(&self) -> f32 {
        self.aabb.span(1)
    }

    pub fn offset_to_ground(&self) -> Length<Feet> {
        feet!(-self.aabb.low(1))
    }

    pub fn aabb(&self) -> &Aabb<f32, 3> {
        &self.aabb
    }
}

pub struct ShapeUploader<'a> {
    name: &'a str,
    palette: &'a Palette,
    catalog: &'a Catalog,
    aabb_min: [f32; 3],
    aabb_max: [f32; 3],
    vert_pool: Vec<Vertex>,
    vertices: Vec<Vertex>,
    active_frame: Option<Frame>,
}

impl<'a> ShapeUploader<'a> {
    pub fn new(name: &'a str, palette: &'a Palette, catalog: &'a Catalog) -> Self {
        Self {
            name,
            palette,
            catalog,
            aabb_min: [f32::INFINITY; 3],
            aabb_max: [f32::NEG_INFINITY; 3],
            vert_pool: Vec::new(),
            vertices: Vec::new(),
            active_frame: None,
        }
    }

    fn align_vertex_pool(&mut self, initial_elems: usize) {
        if initial_elems < self.vert_pool.len() {
            self.vert_pool.truncate(initial_elems);
            return;
        }

        let pad_count = initial_elems - self.vert_pool.len();
        for _ in 0..pad_count {
            self.vert_pool.push(Default::default());
        }
    }

    fn load_vertex_buffer(
        &mut self,
        buffer_properties: &HashMap<usize, BufferProps>,
        vert_buf: &VertexBuf,
    ) -> u32 {
        self.align_vertex_pool(vert_buf.buffer_target_offset());
        let props = buffer_properties
            .get(&vert_buf.at_offset())
            .cloned()
            .unwrap_or_else(|| BufferProps {
                context: "Static".to_owned(),
                flags: VertexFlags::STATIC,
                xform_id: MAX_XFORM_ID,
            });
        for v in vert_buf.vertices() {
            let position = [f32::from(v[0]), f32::from(v[2]), f32::from(-v[1])];
            for (i, &p) in position.iter().enumerate() {
                if p > self.aabb_max[i] {
                    self.aabb_max[i] = p;
                }
                if p < self.aabb_min[i] {
                    self.aabb_min[i] = p;
                }
            }
            self.vert_pool.push(Vertex {
                // Color and Tex Coords will be filled out by the
                // face when we move this into the verts list.
                color: [0.75f32, 0.5f32, 0f32, 1f32],
                tex_coord: [0f32, 0f32],
                // Base position, flags, and the xform are constant
                // for this entire buffer, independent of the face.
                position,
                flags0: (props.flags.bits() & 0xFFFF_FFFF) as u32,
                flags1: (props.flags.bits() >> 32) as u32,
                xform_id: props.xform_id,
            });
        }
        trace!(
            "Loaded VxBuf {} with xform_id: {}",
            props.context,
            props.xform_id
        );
        props.xform_id
    }

    fn push_facet(&mut self, facet: &Facet, override_flags: Option<VertexFlags>) -> Result<()> {
        // Load all vertices in this facet into the vertex upload
        // buffer, copying in the color and texture coords for each
        // face. The layout appears to be for triangle fans.
        for i in 2..facet.indices.len() {
            // Given that most facets are very short strips, and we
            // need to copy the vertices anyway, it's not *that*
            // must worse to just copy the tris over instead of
            // trying to get strips or fans working.
            // TODO: use triangle fans directly
            let js = [0, i - 1, i];
            for j in &js {
                let index = facet.indices[*j] as usize;
                let tex_coord = if facet.flags.contains(FacetFlags::HAVE_TEXCOORDS) {
                    facet.tex_coords[*j]
                } else {
                    [0, 0]
                };

                if index >= self.vert_pool.len() {
                    trace!(
                        "skipping out-of-bounds index at {} of {}",
                        index,
                        self.vert_pool.len(),
                    );
                    continue;
                }
                let mut v = self.vert_pool[index];
                if let Some(flags) = override_flags {
                    v.flags0 = (flags.bits() & 0xFFFF_FFFF) as u32;
                    v.flags1 = (flags.bits() >> 32) as u32;
                }
                v.color = self.palette.rgba_f32(facet.color as usize)?;
                if facet.flags.contains(FacetFlags::FILL_BACKGROUND)
                    || facet.flags.contains(FacetFlags::UNK1)
                    || facet.flags.contains(FacetFlags::UNK5)
                {
                    v.flags0 |= (VertexFlags::BLEND_TEXTURE.bits() & 0xFFFF_FFFF) as u32;
                }
                if facet.flags.contains(FacetFlags::HAVE_TEXCOORDS) {
                    ensure!(
                        self.active_frame.is_some(),
                        "no frame active at facet with texcoords defined"
                    );
                    let frame = self.active_frame.as_ref().unwrap();
                    v.tex_coord = frame.tex_coord_at(tex_coord);
                }
                self.vertices.push(v);
            }
        }
        Ok(())
    }

    // Scan the code segment for references and cross-reference them with trampolines.
    // Return all references to trampolines in the code segment, by name.
    fn find_external_references<'b>(
        x86: &X86Code,
        sh: &'b RawShape,
    ) -> HashMap<&'b str, &'b X86Trampoline> {
        let mut out = HashMap::new();
        for instr in &x86.bytecode.instrs {
            for operand in &instr.operands {
                if let i386::Operand::Memory(memref) = operand {
                    if let Ok(tramp) = sh.lookup_trampoline_by_offset(
                        memref.displacement.wrapping_sub(SHAPE_LOAD_BASE as i32) as u32,
                    ) {
                        out.insert(tramp.name.as_str(), tramp);
                    }
                }
            }
        }
        out
    }

    fn find_external_calls<'b>(
        x86: &X86Code,
        sh: &'b RawShape,
    ) -> Result<HashMap<&'b str, &'b X86Trampoline>> {
        let mut out = HashMap::new();
        let mut push_value = 0;
        for instr in &x86.bytecode.instrs {
            if instr.memonic == i386::Memonic::Push {
                if let i386::Operand::Imm32s(v) = instr.operands[0] {
                    push_value = (v as u32).wrapping_sub(SHAPE_LOAD_BASE);
                }
            }
            if instr.memonic == i386::Memonic::Return {
                let tramp = sh.lookup_trampoline_by_offset(push_value)?;
                out.insert(tramp.name.as_str(), tramp);
            }
        }
        Ok(out)
    }

    fn maybe_update_buffer_properties(
        name: &str,
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        prop_man: &mut BufferPropsManager,
        transformers: &mut Vec<Transformer>,
    ) -> Result<()> {
        let next_instr = pc.relative_instr(1, sh);

        if next_instr.magic() == "Unmask" {
            return Self::handle_unmask_property(pc, x86, sh, prop_man);
        }

        if next_instr.magic() == "XformUnmask" {
            return Self::handle_transformer_property(name, pc, x86, sh, prop_man, transformers);
        }

        // TODO: figure out what is landing here and see how important it is.

        Ok(())
    }

    fn handle_unmask_property(
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        prop_man: &mut BufferPropsManager,
    ) -> Result<()> {
        let memrefs = Self::find_external_references(x86, sh);
        ensure!(
            memrefs.len() == 1,
            "expected unmask with only one parameter"
        );
        let (&name, trampoline) = memrefs.iter().next().expect("checked next");
        if TOGGLE_TABLE.contains_key(name) {
            Self::update_buffer_properties_for_toggle(trampoline, pc, x86, sh, prop_man)?;
        } else if name == "brentObjId" {
            let callrefs = Self::find_external_calls(x86, sh)?;
            ensure!(callrefs.len() == 2, "expected one call");
            ensure!(
                callrefs.contains_key("do_start_interp"),
                "expected call to do_start_interp"
            );
            ensure!(
                callrefs.contains_key("@HARDNumLoaded@8"),
                "expected call to @HARDNumLoaded@8"
            );
            Self::update_buffer_properties_for_num_loaded(trampoline, pc, x86, sh, prop_man)?;
        } else {
            bail!("unknown memory read: {}", name)
        }
        Ok(())
    }

    fn update_buffer_properties_for_toggle(
        trampoline: &X86Trampoline,
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        prop_man: &mut BufferPropsManager,
    ) -> Result<()> {
        let unmask = pc.relative_instr(1, sh);
        let trailer = pc.relative_instr(2, sh);
        ensure!(unmask.magic() == "Unmask", "expected unmask after flag x86");
        ensure!(trailer.magic() == "F0", "expected code after unmask");

        let mut interp = i386::Interpreter::new();
        interp.add_code(x86.bytecode.clone());
        interp.add_code(trailer.unwrap_x86()?.bytecode.clone());
        let do_start_interp = sh.lookup_trampoline_by_name("do_start_interp")?;
        interp.add_trampoline(do_start_interp.mem_location, &do_start_interp.name, 1);

        for &(value, flags) in &TOGGLE_TABLE[trampoline.name.as_str()] {
            interp.map_value(trampoline.mem_location, value);
            let exit_info = interp.interpret(x86.code_offset(SHAPE_LOAD_BASE))?;
            let (name, args) = exit_info.ok_trampoline()?;
            ensure!(name == "do_start_interp", "unexpected trampoline return");
            ensure!(args.len() == 1, "unexpected arg count");
            if unmask.at_offset() == args[0].wrapping_sub(SHAPE_LOAD_BASE) as usize {
                prop_man.add_or_update_toggle_flags(
                    unmask.unwrap_unmask_target()?,
                    flags,
                    &trampoline.name,
                );
            }
            interp.unmap_value(trampoline.mem_location);
        }

        Ok(())
    }

    fn update_buffer_properties_for_num_loaded(
        brent_obj_id: &X86Trampoline,
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        prop_man: &mut BufferPropsManager,
    ) -> Result<()> {
        ensure!(
            brent_obj_id.name == "brentObjId",
            "expected trampoline to be brentObjId"
        );

        let unmask = pc.relative_instr(1, sh);
        let trailer = pc.relative_instr(2, sh);
        ensure!(unmask.magic() == "Unmask", "expected unmask after flag x86");
        ensure!(trailer.magic() == "F0", "expected code after unmask");

        let mut interp = i386::Interpreter::new();
        interp.add_code(x86.bytecode.clone());
        interp.add_code(trailer.unwrap_x86()?.bytecode.clone());
        interp.map_value(brent_obj_id.mem_location, 0x60000);
        let do_start_interp = sh.lookup_trampoline_by_name("do_start_interp")?;
        interp.add_trampoline(do_start_interp.mem_location, &do_start_interp.name, 1);
        let num_loaded = sh.lookup_trampoline_by_name("@HARDNumLoaded@8")?;
        interp.add_trampoline(num_loaded.mem_location, &num_loaded.name, 1);

        for &(value, flags) in &TOGGLE_TABLE["_SAMcount"] {
            let exit_info = interp.interpret(x86.code_offset(0xAA00_0000u32))?;
            let (name, args) = exit_info.ok_trampoline()?;
            ensure!(name == "@HARDNumLoaded@8", "unexpected num_loaded request");
            ensure!(args.len() == 1, "unexpected arg count");
            interp.set_register_value(i386::Reg::EAX, value);

            let exit_info = interp.interpret(interp.eip())?;
            let (name, args) = exit_info.ok_trampoline()?;
            ensure!(name == "do_start_interp", "unexpected trampoline return");
            ensure!(args.len() == 1, "unexpected arg count");
            if unmask.at_offset() == args[0].wrapping_sub(SHAPE_LOAD_BASE) as usize {
                prop_man.add_or_update_toggle_flags(
                    unmask.unwrap_unmask_target()?,
                    flags,
                    "@HARDNumLoaded@8",
                );
            }
        }
        Ok(())
    }

    fn handle_transformer_property(
        name: &str,
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        prop_man: &mut BufferPropsManager,
        transformers: &mut Vec<Transformer>,
    ) -> Result<()> {
        let xform = pc.relative_instr(1, sh);
        let maybe_trailer = pc.relative_instr(2, sh);
        ensure!(xform.magic() == "XformUnmask", "expected xform after x86");

        let mut interp = Interpreter::new();
        interp.add_code(x86.bytecode.clone());
        if let Instr::X86Code(trailer) = maybe_trailer {
            interp.add_code(trailer.bytecode.clone());
        }
        let do_start_interp = sh.lookup_trampoline_by_name("do_start_interp")?;
        interp.add_trampoline(do_start_interp.mem_location, &do_start_interp.name, 1);

        let calls = Self::find_external_calls(x86, sh)?;
        let mut calls = calls.keys().cloned().collect::<Vec<&str>>();
        calls.sort_unstable();

        let memrefs = Self::find_external_references(x86, sh);
        let mut reads = memrefs.keys().cloned().collect::<Vec<&str>>();
        reads.sort_unstable();

        //println!("MEMREFS: {:?}", memrefs);

        let (mask, inputs) = if reads == ["_currentTicks"] && calls == ["do_start_interp"] {
            (
                VertexFlags::STATIC,
                vec![TransformInput::CurrentTicks(
                    memrefs["_currentTicks"].mem_location,
                )],
            )
        } else if reads == ["_PLgearDown"] && calls == ["do_start_interp"] {
            (
                VertexFlags::GEAR_DOWN,
                vec![TransformInput::GearDown(
                    memrefs["_PLgearDown"].mem_location,
                )],
            )
        } else if reads == ["_PLgearDown", "_PLgearPos"] && calls == ["do_start_interp"] {
            (
                VertexFlags::GEAR_DOWN,
                vec![
                    TransformInput::GearPosition(memrefs["_PLgearPos"].mem_location),
                    TransformInput::GearDown(memrefs["_PLgearDown"].mem_location),
                ],
            )
        } else if reads == ["_PLbayDoorPos", "_PLbayOpen"] && calls == ["do_start_interp"] {
            (
                VertexFlags::BAY_OPEN,
                vec![
                    TransformInput::BayPosition(memrefs["_PLbayDoorPos"].mem_location),
                    TransformInput::BayOpen(memrefs["_PLbayOpen"].mem_location),
                ],
            )
        } else if reads == ["_PLcanardPos"] && calls == ["do_start_interp"] {
            // Actually thrust vector
            (
                VertexFlags::STATIC,
                vec![TransformInput::CanardPosition(
                    memrefs["_PLcanardPos"].mem_location,
                )],
            )
        } else if reads == ["_PLafterBurner", "_PLcanardPos"] && calls == ["do_start_interp"] {
            (
                VertexFlags::AFTERBURNER_ON,
                vec![
                    TransformInput::CanardPosition(memrefs["_PLcanardPos"].mem_location),
                    TransformInput::AfterBurner(memrefs["_PLafterBurner"].mem_location),
                ],
            )
        } else if reads == ["_PLafterBurner", "_PLcanardPos", "_PLvtOn"]
            && calls == ["do_start_interp"]
        {
            (
                VertexFlags::AFTERBURNER_ON,
                vec![
                    TransformInput::CanardPosition(memrefs["_PLcanardPos"].mem_location),
                    TransformInput::AfterBurner(memrefs["_PLafterBurner"].mem_location),
                    TransformInput::VerticalOn(memrefs["_PLvtOn"].mem_location),
                ],
            )
        } else if reads == ["_PLvtAngle"] && calls == ["do_start_interp"] {
            // Only used in FA:V22.SH
            (
                VertexFlags::STATIC,
                vec![TransformInput::VerticalAngle(
                    memrefs["_PLvtAngle"].mem_location,
                )],
            )
        } else if reads == ["_PLswingWing"] && calls == ["do_start_interp"] {
            (
                VertexFlags::STATIC,
                vec![TransformInput::SwingWing(
                    memrefs["_PLswingWing"].mem_location,
                )],
            )
        } else {
            println!("UNKNOWN XFORM: {:?} + {:?} in {}", reads, calls, name);
            return Ok(());
        };

        let xform_id =
            prop_man.add_xform_and_flags(xform.unwrap_unmask_target()?, mask, reads[0])?;

        // An immutable copy of the base values for use when running the script.
        // The assumption is that even though these keep state, they are not read
        // from in practice, so their value only matters insofar as they are not
        // written to by the script.
        let xform_base = match xform {
            Instr::XformUnmask(ins) => ins.xform_base,
            Instr::XformUnmask4(ins) => ins.xform_base,
            _ => bail!("not an xform instruction"),
        };

        transformers.push(Transformer {
            xform_id,
            vm: interp,
            code_offset: x86.code_offset(SHAPE_LOAD_BASE),
            data_offset: SHAPE_LOAD_BASE + xform.at_offset() as u32 + 2u32,
            inputs,
            xform_base,
        });

        Ok(())
    }

    pub(crate) fn draw_model(
        &mut self,
        sh: &RawShape,
        analysis: AnalysisResults,
        selection: &DrawSelection,
        atlas: &mut MegaAtlas,
    ) -> Result<(Arc<RwLock<ShapeWidgets>>, Vec<Vertex>)> {
        trace!("ShapeUploader::draw_model: {}", self.name);
        let mut callback = |_pc: &ProgramCounter, instr: &Instr| {
            match instr {
                Instr::JumpToFrame(frame) => {
                    let mask_base = match frame.num_frames() {
                        2 => VertexFlags::ANIM_FRAME_0_2,
                        3 => VertexFlags::ANIM_FRAME_0_3,
                        4 => VertexFlags::ANIM_FRAME_0_4,
                        6 => VertexFlags::ANIM_FRAME_0_6,
                        _ => bail!("only 2, 3, 4, or 6 frame counts supported"),
                    }
                    .bits();
                    for i in 0..frame.num_frames() {
                        // We have already asserted that all frames point to one
                        // face and jump to the same target.
                        let offset = frame.target_for_frame(i);
                        let index = sh.bytes_to_index(offset)?;
                        let target_instr = &sh.instrs[index];
                        let facet = target_instr.unwrap_facet()?;
                        self.push_facet(facet, VertexFlags::from_bits(mask_base << i))?;
                    }
                }

                Instr::TextureRef(texture) => {
                    let filename = texture.filename.to_uppercase();
                    let data = self.catalog.read_name_sync(&filename)?;
                    let pic = Pic::from_bytes(&data)?;
                    self.active_frame = Some(atlas.push(&filename, &pic, &data, self.palette)?);
                }

                Instr::VertexBuf(vert_buf) => {
                    self.load_vertex_buffer(&analysis.prop_man.props, vert_buf);
                }

                Instr::Facet(facet) => {
                    self.push_facet(facet, None)?;
                }

                _ => {}
            }
            Ok(())
        };
        Self::iterate_instructions(sh, selection, &mut callback)?;

        let mut verts = Vec::new();
        mem::swap(&mut verts, &mut self.vertices);

        Ok((
            Arc::new(RwLock::new(ShapeWidgets::new(
                self.name,
                ShapeErrata::from_flags(&analysis),
                analysis.transformers,
                Aabb::new(self.aabb_min, self.aabb_max),
            ))),
            verts,
        ))
    }

    pub(crate) fn analyze_model(
        name: &str,
        sh: &RawShape,
        selection: &DrawSelection,
    ) -> Result<AnalysisResults> {
        let mut result: AnalysisResults = Default::default();
        let mut callback = |pc: &ProgramCounter, instr: &Instr| {
            match instr {
                Instr::JumpToFrame(_) => {
                    result.has_frame_animation = true;
                }
                Instr::X86Code(ref x86) => {
                    Self::maybe_update_buffer_properties(
                        name,
                        pc,
                        x86,
                        sh,
                        &mut result.prop_man,
                        &mut result.transformers,
                    )?;
                }
                Instr::VertexBuf(vert_buf) => {
                    result.prop_man.active_xform_id =
                        if let Some(props) = result.prop_man.props.get(&vert_buf.at_offset()) {
                            props.xform_id
                        } else {
                            MAX_XFORM_ID
                        };
                }
                _ => {}
            };
            Ok(())
        };
        Self::iterate_instructions(sh, selection, &mut callback)?;

        Ok(result)
    }

    fn iterate_instructions<F>(
        sh: &RawShape,
        selection: &DrawSelection,
        callback: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&ProgramCounter, &Instr) -> Result<()>,
    {
        let mut section_close_byte_offset = None;
        let mut damage_model_byte_offset = None;
        let mut end_byte_offset = None;

        let mut pc = ProgramCounter::new(sh.instrs.len());
        while pc.valid() {
            if let Some(byte_offset) = damage_model_byte_offset {
                if pc.matches_byte(byte_offset) && *selection != DrawSelection::DamageModel {
                    pc.set_byte_offset(end_byte_offset.unwrap(), sh)?;
                }
            }
            if let Some(byte_offset) = section_close_byte_offset {
                if pc.matches_byte(byte_offset) {
                    pc.set_byte_offset(end_byte_offset.unwrap(), sh)?;
                }
            }

            let instr = pc.current_instr(sh);
            callback(&pc, instr)?;

            //println!("At: {:3} => {}", pc.instr_offset, instr.show());
            match instr {
                Instr::Header(_) => {}
                Instr::PtrToObjEnd(end) => end_byte_offset = Some(end.end_byte_offset()),
                Instr::EndOfObject(_end) => break,

                Instr::Jump(jump) => {
                    pc.set_byte_offset(jump.target_byte_offset(), sh)?;
                    continue;
                }
                Instr::JumpToDamage(dam) => {
                    damage_model_byte_offset = Some(dam.damage_byte_offset());
                    if *selection == DrawSelection::DamageModel {
                        pc.set_byte_offset(dam.damage_byte_offset(), sh)?;
                        continue;
                    }
                }
                Instr::JumpToDetail(detail) => {
                    section_close_byte_offset = Some(detail.target_byte_offset());
                }
                Instr::JumpToLOD(lod) => {
                    section_close_byte_offset = Some(lod.target_byte_offset());
                }
                Instr::JumpToFrame(frame) => {
                    pc.set_byte_offset(frame.target_for_frame(0), sh)?;
                }

                _ => {}
            }

            pc.advance(sh);
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_vertex_offsets() {
        let _ = Vertex::descriptor();
    }
}
