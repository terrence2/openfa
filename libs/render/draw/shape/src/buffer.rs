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
    buffer_manager::BufferUploadState,
    draw_state::DrawState,
    texture_atlas::{Frame, TextureAtlas},
    UNIFORM_POOL_SIZE,
};
use bitflags::bitflags;
use failure::{bail, ensure, err_msg, Fallible};
use i386::Interpreter;
use image::{ImageBuffer, Rgba};
use lazy_static::lazy_static;
use lib::Library;
use log::trace;
use nalgebra::{Matrix4, Vector3};
use pal::Palette;
use pic::Pic;
use sh::{Facet, FacetFlags, Instr, RawShape, VertexBuf, X86Code, X86Trampoline, SHAPE_LOAD_BASE};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, DeviceLocalBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    image::{Dimensions, ImmutableImage},
    impl_vertex,
    pipeline::GraphicsPipelineAbstract,
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

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
    pub fn displacement(self, offset: usize) -> Fallible<Self> {
        VertexFlags::from_bits(self.bits() << offset).ok_or_else(|| {
            err_msg(format!(
                "offset {} from {:?} did not yield a valid vertex flags",
                offset, self
            ))
        })
    }
}

#[derive(Copy, Clone)]
pub struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    tex_coord: [f32; 2],
    flags0: u32,
    flags1: u32,
    xform_id: u32,
}
impl_vertex!(Vertex, position, color, tex_coord, flags0, flags1, xform_id);

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

#[derive(Copy, Clone)]
pub struct ShapeErrata {
    pub no_upper_aileron: bool,
    // has_toggle_gear: bool,
    // has_toggle_bay: bool,
}

impl ShapeErrata {
    fn from_flags(flags: VertexFlags) -> Self {
        Self {
            // ERRATA: ATFNATO:F22.SH is missing aileron up meshes.
            no_upper_aileron: !(flags & VertexFlags::AILERONS_DOWN).is_empty()
                && (flags & VertexFlags::AILERONS_UP).is_empty(),
        }
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
    fn set_byte_offset(&mut self, next_offset: usize, sh: &RawShape) -> Fallible<()> {
        self.byte_offset = next_offset;
        self.instr_offset = sh.bytes_to_index(next_offset)?;
        ensure!(self.valid(), "pc jumped out of bounds");
        Ok(())
    }

    // Get the current instructions.
    fn current_instr<'a>(&self, sh: &'a RawShape) -> &'a Instr {
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
    ) -> Fallible<u32> {
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
pub struct Transformer {
    xform_id: u32,
    vm: Interpreter,
    code_offset: u32,
    data_offset: u32,
    inputs: Vec<TransformInput>,
    xform_base: [u8; 12],
}

impl Transformer {
    pub fn transform(
        &mut self,
        draw_state: &DrawState,
        start: &Instant,
        now: &Instant,
    ) -> Fallible<[f32; 6]> {
        fn fa2r(d: f32) -> f32 {
            d * std::f32::consts::PI / 8192f32
        }

        let gear_position = draw_state.gear_position() as u32;
        let bay_position = draw_state.bay_position() as u32;
        let thrust_vectoring = draw_state.thrust_vector_position() as i32 as u32;
        let wing_sweep = i32::from(draw_state.wing_sweep_angle()) as u32;
        let vm = &mut self.vm;
        let t = (((*now - *start).as_millis() as u32) >> 4) & 0x0FFF;
        for input in &self.inputs {
            match input {
                TransformInput::CurrentTicks(loc) => {
                    vm.add_read_port(*loc, Box::new(move || t));
                }
                TransformInput::GearPosition(loc) => {
                    vm.add_read_port(*loc, Box::new(move || gear_position));
                }
                TransformInput::GearDown(loc) => {
                    vm.add_read_port(*loc, Box::new(move || 1));
                }
                TransformInput::BayPosition(loc) => {
                    vm.add_read_port(*loc, Box::new(move || bay_position));
                }
                TransformInput::BayOpen(loc) => {
                    vm.add_read_port(*loc, Box::new(move || 1));
                }
                TransformInput::CanardPosition(loc) => {
                    vm.add_read_port(*loc, Box::new(move || thrust_vectoring));
                }
                TransformInput::AfterBurner(loc) => {
                    vm.add_read_port(*loc, Box::new(move || 1));
                }
                TransformInput::VerticalOn(loc) => {
                    vm.add_read_port(*loc, Box::new(move || 0));
                }
                TransformInput::VerticalAngle(loc) => {
                    vm.add_read_port(*loc, Box::new(move || thrust_vectoring));
                }
                TransformInput::SwingWing(loc) => {
                    vm.add_read_port(*loc, Box::new(move || wing_sweep));
                }
            }
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
                TransformInput::CurrentTicks(loc) => vm.remove_read_port(*loc),
                TransformInput::GearPosition(loc) => vm.remove_read_port(*loc),
                TransformInput::GearDown(loc) => vm.remove_read_port(*loc),
                TransformInput::BayPosition(loc) => vm.remove_read_port(*loc),
                TransformInput::BayOpen(loc) => vm.remove_read_port(*loc),
                TransformInput::CanardPosition(loc) => vm.remove_read_port(*loc),
                TransformInput::AfterBurner(loc) => vm.remove_read_port(*loc),
                TransformInput::VerticalOn(loc) => vm.remove_read_port(*loc),
                TransformInput::VerticalAngle(loc) => vm.remove_read_port(*loc),
                TransformInput::SwingWing(loc) => vm.remove_read_port(*loc),
            }
        }
        //Ok(xform)
        Ok(arr)
    }
}

pub struct ShapeBuffer {
    descriptor_set: Arc<dyn DescriptorSet + Send + Sync>,

    // Self contained vm/instructions for how to set up each required transform
    // to draw this shape buffer.
    transformers: Vec<Transformer>,

    // Draw properties based on what's in the shape file.
    errata: ShapeErrata,
}

#[derive(Clone)]
pub struct ShapeBufferRef {
    value: Arc<RefCell<ShapeBuffer>>,
}

impl ShapeBufferRef {
    pub fn new(buffer: ShapeBuffer) -> Self {
        ShapeBufferRef {
            value: Arc::new(RefCell::new(buffer)),
        }
    }

    pub fn apply_animation(
        &mut self,
        draw_state: &DrawState,
        start: &Instant,
        now: &Instant,
    ) -> Fallible<HashMap<u32, [f32; 6]>> {
        let mut xform_states = HashMap::new();
        for transformer in self.value.borrow_mut().transformers.iter_mut() {
            let xform = transformer.transform(draw_state, start, now)?;
            xform_states.insert(transformer.xform_id, xform);
        }
        Ok(xform_states)
    }

    pub fn errata(&self) -> ShapeErrata {
        self.value.borrow().errata
    }

    pub fn descriptor_set_ref(&self) -> Arc<dyn DescriptorSet + Send + Sync> {
        self.value.borrow().descriptor_set.clone()
    }
}

pub struct ShapesBuffer {
    buffers: HashMap<String, ShapeBufferRef>,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
}

impl ShapesBuffer {
    pub fn new(pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>) -> Fallible<Self> {
        trace!("ShapeBuffer::new");

        Ok(Self {
            pipeline,
            buffers: HashMap::new(),
        })
    }

    fn align_vertex_pool(vert_pool: &mut Vec<Vertex>, initial_elems: usize) {
        if initial_elems < vert_pool.len() {
            vert_pool.truncate(initial_elems);
            return;
        }

        let pad_count = initial_elems - vert_pool.len();
        for _ in 0..pad_count {
            vert_pool.push(Default::default());
        }
    }

    fn load_vertex_buffer(
        buffer_properties: &HashMap<usize, BufferProps>,
        vert_buf: &VertexBuf,
        vert_pool: &mut Vec<Vertex>,
    ) -> u32 {
        Self::align_vertex_pool(vert_pool, vert_buf.buffer_target_offset());
        let props = buffer_properties
            .get(&vert_buf.at_offset())
            .cloned()
            .unwrap_or_else(|| BufferProps {
                context: "Static".to_owned(),
                flags: VertexFlags::STATIC,
                xform_id: 0,
            });
        for v in vert_buf.vertices() {
            let v0 = Vector3::new(f32::from(v[0]), f32::from(-v[2]), -f32::from(v[1]));
            vert_pool.push(Vertex {
                // Color and Tex Coords will be filled out by the
                // face when we move this into the verts list.
                color: [0.75f32, 0.5f32, 0f32, 1f32],
                tex_coord: [0f32, 0f32],
                // Base position, flags, and the xform are constant
                // for this entire buffer, independent of the face.
                position: [v0[0], v0[1], v0[2]],
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

    fn push_facet(
        facet: &Facet,
        vert_pool: &[Vertex],
        palette: &Palette,
        active_frame: Option<&Frame>,
        override_flags: Option<VertexFlags>,
        bus: &mut BufferUploadState,
    ) -> Fallible<()> {
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

                if index >= vert_pool.len() {
                    trace!(
                        "skipping out-of-bounds index at {} of {}",
                        index,
                        vert_pool.len(),
                    );
                    continue;
                }
                let mut v = vert_pool[index];
                if let Some(flags) = override_flags {
                    v.flags0 = (flags.bits() & 0xFFFF_FFFF) as u32;
                    v.flags1 = (flags.bits() >> 32) as u32;
                }
                v.color = palette.rgba_f32(facet.color as usize)?;
                if facet.flags.contains(FacetFlags::FILL_BACKGROUND)
                    || facet.flags.contains(FacetFlags::UNK1)
                    || facet.flags.contains(FacetFlags::UNK5)
                {
                    v.flags0 |= (VertexFlags::BLEND_TEXTURE.bits() & 0xFFFF_FFFF) as u32;
                }
                if facet.flags.contains(FacetFlags::HAVE_TEXCOORDS) {
                    assert!(active_frame.is_some());
                    let frame = active_frame.unwrap();
                    v.tex_coord = frame.tex_coord_at(tex_coord);
                }
                bus.push_with_index(v);
            }
        }
        Ok(())
    }

    // Scan the code segment for references and cross-reference them with trampolines.
    // Return all references to trampolines in the code segment, by name.
    fn find_external_references<'a>(
        x86: &X86Code,
        sh: &'a RawShape,
    ) -> HashMap<&'a str, &'a X86Trampoline> {
        let mut out = HashMap::new();
        for instr in &x86.bytecode.borrow().instrs {
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

    fn find_external_calls<'a>(
        x86: &X86Code,
        sh: &'a RawShape,
    ) -> Fallible<HashMap<&'a str, &'a X86Trampoline>> {
        let mut out = HashMap::new();
        let mut push_value = 0;
        for instr in &x86.bytecode.borrow().instrs {
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
    ) -> Fallible<()> {
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
    ) -> Fallible<()> {
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
    ) -> Fallible<()> {
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
            interp.add_read_port(trampoline.mem_location, Box::new(move || value));
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
            interp.remove_read_port(trampoline.mem_location);
        }

        Ok(())
    }

    fn update_buffer_properties_for_num_loaded(
        brent_obj_id: &X86Trampoline,
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        prop_man: &mut BufferPropsManager,
    ) -> Fallible<()> {
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
        interp.add_read_port(brent_obj_id.mem_location, Box::new(move || 0x60000));
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
    ) -> Fallible<()> {
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
        calls.sort();

        let memrefs = Self::find_external_references(x86, sh);
        let mut reads = memrefs.keys().cloned().collect::<Vec<&str>>();
        reads.sort();

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

    fn draw_model(
        &self,
        name: &str,
        sh: &RawShape,
        selection: DrawSelection,
        palette: &Palette,
        atlas: &TextureAtlas,
        bus: &mut BufferUploadState,
    ) -> Fallible<(Vec<Transformer>, ShapeErrata)> {
        // Outputs
        let mut transformers = Vec::new();
        let mut xforms = Vec::new();
        xforms.push(Matrix4::<f32>::identity());

        // State
        let mut prop_man = BufferPropsManager {
            seen_flags: VertexFlags::NONE,
            props: HashMap::new(),
            next_xform_id: 1,
            active_xform_id: 0,
        };
        let mut active_frame = None;
        let mut section_close_byte_offset = None;
        let mut damage_model_byte_offset = None;
        let mut end_byte_offset = None;
        let mut vert_pool = Vec::new();

        let mut pc = ProgramCounter::new(sh.instrs.len());
        while pc.valid() {
            if let Some(byte_offset) = damage_model_byte_offset {
                if pc.matches_byte(byte_offset) && selection != DrawSelection::DamageModel {
                    pc.set_byte_offset(end_byte_offset.unwrap(), sh)?;
                }
            }
            if let Some(byte_offset) = section_close_byte_offset {
                if pc.matches_byte(byte_offset) {
                    pc.set_byte_offset(end_byte_offset.unwrap(), sh)?;
                }
            }

            let instr = pc.current_instr(sh);
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
                    if selection == DrawSelection::DamageModel {
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
                        Self::push_facet(
                            facet,
                            &vert_pool,
                            palette,
                            active_frame,
                            VertexFlags::from_bits(mask_base << i),
                            bus,
                        )?;
                    }
                    pc.set_byte_offset(frame.target_for_frame(0), sh)?;
                }

                Instr::X86Code(ref x86) => {
                    Self::maybe_update_buffer_properties(
                        name,
                        &pc,
                        x86,
                        sh,
                        &mut prop_man,
                        &mut transformers,
                    )?;
                }

                Instr::TextureRef(texture) => {
                    active_frame = Some(&atlas.frames[&texture.filename]);
                }

                Instr::VertexBuf(vert_buf) => {
                    prop_man.active_xform_id =
                        Self::load_vertex_buffer(&prop_man.props, vert_buf, &mut vert_pool);
                }

                Instr::Facet(facet) => {
                    Self::push_facet(facet, &vert_pool, palette, active_frame, None, bus)?;
                }
                _ => {}
            }

            pc.advance(sh);
        }

        Ok((transformers, ShapeErrata::from_flags(prop_man.seen_flags)))
    }

    // We take a pre-pass to load all textures so that we can pre-allocate
    // the full texture atlas up front and deliver frames for translating
    // texture coordinates in the main loop below. Note that we could
    // probably get away with creating frames on the fly if we want to cut
    // out this pass and load the atlas after the fact.
    pub fn upload_atlas(
        sh: &RawShape,
        palette: &Palette,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<(TextureAtlas, Arc<ImmutableImage<Format>>, Box<GpuFuture>)> {
        let texture_filenames = sh.all_textures();
        let mut texture_headers = Vec::new();
        for filename in texture_filenames {
            let data = lib.load(&filename.to_uppercase())?;
            texture_headers.push((filename.to_owned(), Pic::from_bytes(&data)?, data));
        }
        let atlas = TextureAtlas::from_raw_data(&palette, texture_headers)?;
        let (texture, tex_future) = Self::upload_texture_rgba(window, atlas.img.to_rgba())?;
        Ok((atlas, texture, tex_future))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upload_shape(
        &mut self,
        name: &str,
        sh: &RawShape,
        selection: DrawSelection,
        bus: &mut BufferUploadState,
        uniform_buffer: Arc<DeviceLocalBuffer<[f32; UNIFORM_POOL_SIZE]>>,
        palette: &Palette,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<ShapeBufferRef> {
        if self.buffers.contains_key(name) {
            return Ok(self.buffers[name].clone());
        }

        let (atlas, texture, future) = Self::upload_atlas(sh, palette, lib, window)?;
        let (transformers, errata) = self.draw_model(name, sh, selection, palette, &atlas, bus)?;

        future.then_signal_fence_and_flush()?.cleanup_finished();
        let descriptor_set = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), Self::make_sampler(window.device())?)?
                .add_buffer(uniform_buffer.clone())?
                .build()?,
        );

        let buffer = ShapeBuffer {
            descriptor_set,
            transformers,
            errata,
        };
        let buffer_ref = ShapeBufferRef::new(buffer);

        self.buffers.insert(name.to_owned(), buffer_ref.clone());
        Ok(buffer_ref)
    }

    fn upload_texture_rgba(
        window: &GraphicsWindow,
        image_buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
    ) -> Fallible<(Arc<ImmutableImage<Format>>, Box<GpuFuture>)> {
        let image_dim = image_buf.dimensions();
        let image_data = image_buf.into_raw().clone();

        let dimensions = Dimensions::Dim2d {
            width: image_dim.0,
            height: image_dim.1,
        };
        let (texture, tex_future) = ImmutableImage::from_iter(
            image_data.iter().cloned(),
            dimensions,
            Format::R8G8B8A8Unorm,
            window.queue(),
        )?;
        Ok((texture, Box::new(tex_future) as Box<GpuFuture>))
    }

    fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
            Filter::Nearest,
            Filter::Nearest,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        )?;

        Ok(sampler)
    }
}
