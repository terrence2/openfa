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
use crate::sh::texture_atlas::{Frame, TextureAtlas};
use bitflags::bitflags;
use failure::{bail, ensure, Fallible};
use i386::ExitInfo;
use image::{ImageBuffer, Rgba};
use lazy_static::lazy_static;
use lib::Library;
use log::trace;
use nalgebra::{Matrix4, Vector3, Vector4};
use pal::Palette;
use pic::Pic;
use sh::{Facet, FacetFlags, Instr, RawShape, VertexBuf, X86Code, X86Trampoline, SHAPE_LOAD_BASE};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    framebuffer::Subpass,
    image::{Dimensions, ImmutableImage},
    impl_vertex,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

bitflags! {
    struct VertexFlags: u64 {
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
        const BAY_DOOR_CLOSED      = 0x0000_0000_0000_4000;
        const BAY_DOOR_OPEN        = 0x0000_0000_0000_8000;
        const RUDDER_CENTER        = 0x0000_0000_0001_0000;
        const RUDDER_LEFT          = 0x0000_0000_0002_0000;
        const RUDDER_RIGHT         = 0x0000_0000_0004_0000;
        const LEFT_AILERON_CENTER  = 0x0000_0000_0008_0000;
        const LEFT_AILERON_UP      = 0x0000_0000_0010_0000;
        const LEFT_AILERON_DOWN    = 0x0000_0000_0020_0000;
        const RIGHT_AILERON_CENTER = 0x0000_0000_0040_0000;
        const RIGHT_AILERON_UP     = 0x0000_0000_0080_0000;
        const RIGHT_AILERON_DOWN   = 0x0000_0000_0100_0000;

        const ANIM_FRAME_0         = 0x0000_0001_0000_0000;
        const ANIM_FRAME_1         = 0x0000_0002_0000_0000;
        const ANIM_FRAME_2         = 0x0000_0004_0000_0000;
        const ANIM_FRAME_3         = 0x0000_0008_0000_0000;
        const ANIM_FRAME_4         = 0x0000_0010_0000_0000;
        const ANIM_FRAME_5         = 0x0000_0020_0000_0000;

        const SAM_COUNT_0          = 0x0000_0040_0000_0000;
        const SAM_COUNT_1          = 0x0000_0080_0000_0000;
        const SAM_COUNT_2          = 0x0000_0100_0000_0000;
        const SAM_COUNT_3          = 0x0000_0200_0000_0000;
    }
}

#[derive(Copy, Clone)]
struct Vertex {
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

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec3 position;
            layout(location = 1) in vec4 color;
            layout(location = 2) in vec2 tex_coord;
            layout(location = 3) in uint flags0;
            layout(location = 4) in uint flags1;
            layout(location = 5) in uint xform_id;

            layout(push_constant) uniform PushConstantData {
              mat4 view;
              mat4 projection;
              uint flag_mask0;
              uint flag_mask1;
            } pc;

            layout(location = 0) smooth out vec4 v_color;
            layout(location = 1) smooth out vec2 v_tex_coord;
            layout(location = 2) flat out uint f_flags0;
            layout(location = 3) flat out uint f_flags1;

            void main() {
                gl_Position = pc.projection * pc.view * vec4(position, 1.0);
                v_color = color;
                v_tex_coord = tex_coord;
                f_flags0 = flags0 & pc.flag_mask0;
                f_flags1 = flags1 & pc.flag_mask1;
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
        src: "
            #version 450

            layout(location = 0) smooth in vec4 v_color;
            layout(location = 1) smooth in vec2 v_tex_coord;
            layout(location = 2) flat in uint f_flags0;
            layout(location = 3) flat in uint f_flags1;

            layout(location = 0) out vec4 f_color;

            layout(set = 0, binding = 0) uniform sampler2D tex;

            void main() {
                if ((f_flags0 & 0xFFFFFFFE) == 0 && f_flags1 == 0) {
                    discard;
                } else if (v_tex_coord.x == 0.0) {
                    f_color = v_color;
                } else {
                    vec4 tex_color = texture(tex, v_tex_coord);

                    if ((f_flags0 & 1) == 1) {
                        f_color = vec4((1.0 - tex_color[3]) * v_color.xyz + tex_color[3] * tex_color.xyz, 1.0);
                    } else {
                        if (tex_color.a < 0.5)
                            discard;
                        else
                            f_color = tex_color;
                    }
                }
            }
            "
    }
}

impl vs::ty::PushConstantData {
    fn new() -> Self {
        Self {
            view: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
            projection: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
            flag_mask0: 0xFFFF_FFFF,
            flag_mask1: 0xFFFF_FFFF,
        }
    }

    fn set_view(&mut self, mat: Matrix4<f32>) {
        self.view[0][0] = mat[0];
        self.view[0][1] = mat[1];
        self.view[0][2] = mat[2];
        self.view[0][3] = mat[3];
        self.view[1][0] = mat[4];
        self.view[1][1] = mat[5];
        self.view[1][2] = mat[6];
        self.view[1][3] = mat[7];
        self.view[2][0] = mat[8];
        self.view[2][1] = mat[9];
        self.view[2][2] = mat[10];
        self.view[2][3] = mat[11];
        self.view[3][0] = mat[12];
        self.view[3][1] = mat[13];
        self.view[3][2] = mat[14];
        self.view[3][3] = mat[15];
    }

    fn set_projection(&mut self, mat: &Matrix4<f32>) {
        self.projection[0][0] = mat[0];
        self.projection[0][1] = mat[1];
        self.projection[0][2] = mat[2];
        self.projection[0][3] = mat[3];
        self.projection[1][0] = mat[4];
        self.projection[1][1] = mat[5];
        self.projection[1][2] = mat[6];
        self.projection[1][3] = mat[7];
        self.projection[2][0] = mat[8];
        self.projection[2][1] = mat[9];
        self.projection[2][2] = mat[10];
        self.projection[2][3] = mat[11];
        self.projection[3][0] = mat[12];
        self.projection[3][1] = mat[13];
        self.projection[3][2] = mat[14];
        self.projection[3][3] = mat[15];
    }
}

#[derive(Clone)]
pub struct ShInstance {
    push_constants: vs::ty::PushConstantData,
    pds: Arc<dyn DescriptorSet + Send + Sync>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
}

#[derive(Clone, Eq, PartialEq)]
enum DrawSelection {
    DamageModel,
    NormalModel,
}

#[derive(Clone, Eq, PartialEq)]
pub struct DrawMode {
    pub range: Option<[usize; 2]>,
    pub damaged: bool,
    pub closeness: usize,
    pub frame_number: usize,
    pub detail: u16,

    pub gear_position: Option<u32>,
    pub bay_position: Option<u32>,
    pub flaps_down: bool,
    pub airbrake_extended: bool,
    pub hook_extended: bool,
    pub afterburner_enabled: bool,
    pub rudder_position: i32,
    pub sam_count: u32,
}

impl DrawMode {
    fn to_mask(&self) -> Fallible<u64> {
        let mut mask = VertexFlags::STATIC | VertexFlags::BLEND_TEXTURE;

        mask |= if self.flaps_down {
            VertexFlags::LEFT_FLAP_DOWN | VertexFlags::RIGHT_FLAP_DOWN
        } else {
            VertexFlags::LEFT_FLAP_UP | VertexFlags::RIGHT_FLAP_UP
        };

        mask |= if self.airbrake_extended {
            VertexFlags::BRAKE_EXTENDED
        } else {
            VertexFlags::BRAKE_RETRACTED
        };

        mask |= if self.hook_extended {
            VertexFlags::HOOK_EXTENDED
        } else {
            VertexFlags::HOOK_RETRACTED
        };

        mask |= if self.rudder_position < 0 {
            VertexFlags::RUDDER_RIGHT
        } else if self.rudder_position > 0 {
            VertexFlags::RUDDER_LEFT
        } else {
            VertexFlags::RUDDER_CENTER
        };

        // FIXME: add aileron inputs

        mask |= if self.afterburner_enabled {
            VertexFlags::AFTERBURNER_ON
        } else {
            VertexFlags::AFTERBURNER_OFF
        };

        mask |= if self.gear_position.is_some() {
            VertexFlags::GEAR_DOWN
        } else {
            VertexFlags::GEAR_UP
        };

        mask |= match self.sam_count {
            0 => VertexFlags::SAM_COUNT_0,
            1 => VertexFlags::SAM_COUNT_0 | VertexFlags::SAM_COUNT_1,
            2 => VertexFlags::SAM_COUNT_0 | VertexFlags::SAM_COUNT_1 | VertexFlags::SAM_COUNT_2,
            3 => {
                VertexFlags::SAM_COUNT_0
                    | VertexFlags::SAM_COUNT_1
                    | VertexFlags::SAM_COUNT_2
                    | VertexFlags::SAM_COUNT_3
            }
            _ => bail!("expected sam count < 3"),
        };

        Ok(mask.bits())
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
            "_PLgearDown",
            vec![(0, VertexFlags::GEAR_UP), (1, VertexFlags::GEAR_DOWN)],
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

#[derive(Clone, Copy)]
struct BufferProperties {
    flags: VertexFlags,
    xform_id: u32,
}

pub struct ShRenderer {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    instance: Option<ShInstance>,
}

const INST_BASE: u32 = 0x0000_4000;

impl ShRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        trace!("ShRenderer::new");

        let vs = vs::Shader::load(window.device())?;
        let fs = fs::Shader::load(window.device())?;

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(vs.main_entry_point(), ())
                .triangle_list()
                .cull_mode_back()
                .front_face_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
                .depth_stencil(DepthStencil {
                    depth_write: true,
                    depth_compare: Compare::GreaterOrEqual,
                    depth_bounds_test: DepthBounds::Disabled,
                    stencil_front: Default::default(),
                    stencil_back: Default::default(),
                })
                .blend_alpha_blending()
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        );
        Ok(ShRenderer {
            pipeline,
            instance: None,
        })
    }

    pub fn set_projection(&mut self, projection: &Matrix4<f32>) {
        self.instance
            .as_mut()
            .unwrap()
            .push_constants
            .set_projection(projection);
    }

    pub fn set_view(&mut self, view: Matrix4<f32>) {
        self.instance
            .as_mut()
            .unwrap()
            .push_constants
            .set_view(view);
    }

    pub fn set_plane_state(&mut self, mode: &DrawMode) -> Fallible<()> {
        let full_mask = mode.to_mask()?;
        self.instance.as_mut().unwrap().push_constants.flag_mask0 =
            (full_mask & 0xFFFF_FFFF) as u32;
        self.instance.as_mut().unwrap().push_constants.flag_mask1 = (full_mask >> 32) as u32;
        Ok(())
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
        buffer_properties: &HashMap<usize, BufferProperties>,
        vert_buf: &VertexBuf,
        vert_pool: &mut Vec<Vertex>,
    ) {
        Self::align_vertex_pool(vert_pool, vert_buf.buffer_target_offset());
        let props = buffer_properties
            .get(&vert_buf.at_offset())
            .cloned()
            .unwrap_or_else(|| BufferProperties {
                flags: VertexFlags::STATIC,
                xform_id: 0,
            });
        println!("VERTEX FLAGS: {:?}", props.flags);
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
    }

    fn push_facet(
        facet: &Facet,
        vert_pool: &Vec<Vertex>,
        palette: &Palette,
        active_frame: &Option<&Frame>,
        verts: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) -> Fallible<()> {
        // Load all vertices in this facet into the vertex upload
        // buffer, copying in the color and texture coords for each
        // face. The layout appears to be for triangle fans.
        let mut v_base = verts.len() as u32;
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
                verts.push(v);
                indices.push(v_base);
                v_base += 1;
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

    fn maybe_update_buffer_properties(
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        props: &mut HashMap<usize, BufferProperties>,
    ) -> Fallible<usize> {
        let memrefs = Self::find_external_references(x86, sh);
        let next_instr = pc.relative_instr(1, sh);
        if next_instr.magic() == "Unmask" {
            ensure!(
                memrefs.len() == 1,
                "expected unmask with only one parameter"
            );
            let (name, trampoline) = memrefs.iter().next().unwrap();
            if TOGGLE_TABLE.contains_key(name) {
                Self::update_buffer_properties_for_toggle(trampoline, pc, x86, sh, props)?;
            } else {
                bail!("unknown toggle: {}", name);
            }
        }
        /*
        if memrefs.len() == 1 {
            let (name, trampoline) = memrefs.iter().next().unwrap();
            if TOGGLE_TABLE.contains_key(name) {
                if next_instr.magic() == "Unmask" {
                    Self::update_buffer_properties_for_toggle(
                        trampoline,
                        pc,
                        x86,
                        sh,
                        buffer_properties,
                    )?;
                } else {
                    // It's weird to have a toggle with an XformUnmask, but there are a handful
                    // of shapes that have an un-mutated transform they need.
                    ensure!(
                        next_instr.magic() == "XformUnmask",
                        "single input must be unmask or xformunmask"
                    );
                    // TODO: implement me
                }
                return Ok(2);
            } else if SKIP_TABLE.contains(name) {
            } else {
            }
        } else if memrefs.len() == 2 {

        }
        */

        Ok(0)
    }

    fn update_buffer_properties_for_toggle(
        trampoline: &X86Trampoline,
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        props: &mut HashMap<usize, BufferProperties>,
    ) -> Fallible<()> {
        let unmask = pc.relative_instr(1, sh);
        let trailer = pc.relative_instr(2, sh);
        ensure!(unmask.magic() == "Unmask", "expected unmask after flag x86");
        ensure!(trailer.magic() == "F0", "expected code after unmask");

        let mut interp = i386::Interpreter::new();
        let do_start_interp = sh.lookup_trampoline_by_name("do_start_interp")?;
        interp.add_code(&x86.bytecode);
        interp.add_code(&trailer.unwrap_x86()?.bytecode);
        interp.add_trampoline(do_start_interp.mem_location, &do_start_interp.name, 1);

        for &(value, flags) in &TOGGLE_TABLE[trampoline.name.as_str()] {
            interp.add_read_port(trampoline.mem_location, Box::new(move || value));
            let exit_info = interp.interpret(x86.code_offset(0xAA00_0000u32)).unwrap();
            let (name, args) = exit_info.ok_trampoline()?;
            ensure!(name == "do_start_interp", "unexpected trampoline return");
            ensure!(args.len() == 1, "unexpected arg count");
            if unmask.at_offset() == args[0].wrapping_sub(SHAPE_LOAD_BASE) as usize {
                let tgt = unmask.unwrap_unmask_target()?;
                if !props.contains_key(&tgt) {
                    props.insert(tgt, BufferProperties { flags, xform_id: 0 });
                } else {
                    let p = props.get_mut(&tgt).unwrap();
                    p.flags |= flags;
                }
            }
            interp.remove_read_port(trampoline.mem_location);
        }

        Ok(())
    }

    fn draw_model(
        &self,
        sh: &RawShape,
        palette: &Palette,
        atlas: &TextureAtlas,
        selection: DrawSelection,
        window: &GraphicsWindow,
    ) -> Fallible<ShInstance> {
        // Outputs
        let mut verts = Vec::new();
        let mut indices = Vec::new();
        let mut xforms = Vec::new();
        xforms.push(Matrix4::<f32>::identity());

        // State
        let mut active_frame = None;
        let _active_xform_id = 0;
        let mut buffer_properties: HashMap<usize, BufferProperties> = HashMap::new();
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
            println!("At: {:3} => {}", pc.instr_offset, instr.show());
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

                Instr::X86Code(ref x86) => {
                    let advance_cnt =
                        Self::maybe_update_buffer_properties(&pc, x86, sh, &mut buffer_properties)?;
                    for _ in 0..advance_cnt {
                        pc.advance(sh);
                    }
                }

                Instr::TextureRef(texture) => {
                    active_frame = Some(&atlas.frames[&texture.filename]);
                }

                Instr::VertexBuf(vert_buf) => {
                    Self::load_vertex_buffer(&buffer_properties, vert_buf, &mut vert_pool);
                }

                Instr::Facet(facet) => {
                    Self::push_facet(
                        facet,
                        &vert_pool,
                        palette,
                        &active_frame,
                        &mut verts,
                        &mut indices,
                    )?;
                }
                _ => {}
            }

            pc.advance(sh);
        }

        trace!(
            "uploading vertex buffer with {} bytes",
            std::mem::size_of::<Vertex>() * verts.len()
        );
        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

        trace!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        let (texture, tex_future) = Self::upload_texture_rgba(window, atlas.img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        Ok(ShInstance {
            push_constants: vs::ty::PushConstantData::new(),
            pds,
            vertex_buffer,
            index_buffer,
        })
    }

    pub fn add_shape_to_render(
        &mut self,
        palette: &Palette,
        sh: &RawShape,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        // We take a pre-pass to load all textures so that we can pre-allocate
        // the full texture atlas up front and deliver frames for translating
        // texture coordinates in the main loop below. Note that we could
        // probably get away with creating frames on the fly if we want to cut
        // out this pass and load the atlas after the fact.
        let texture_filenames = sh.all_textures();
        let mut texture_headers = Vec::new();
        for filename in texture_filenames {
            let data = lib.load(&filename.to_uppercase())?;
            texture_headers.push((filename.to_owned(), Pic::from_bytes(&data)?, data));
        }
        let atlas = TextureAtlas::from_raw_data(&palette, texture_headers)?;

        let main = self.draw_model(sh, palette, &atlas, DrawSelection::NormalModel, window)?;
        //let damaged = self.draw_model(sh, palette, &atlas, DrawSelection::DamageModel, window)?;
        self.instance = Some(main);

        Ok(())
    }

    #[allow(clippy::cyclomatic_complexity, clippy::too_many_arguments)]
    pub fn legacy_add_shape_to_render(
        &mut self,
        system_palette: Arc<Palette>,
        sh: &RawShape,
        stop_at_offset: usize,
        draw_mode: &DrawMode,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        let texture_filenames = sh.all_textures();
        let mut texture_headers = Vec::new();
        for filename in texture_filenames {
            let data = lib.load(&filename.to_uppercase())?;
            texture_headers.push((filename.to_owned(), Pic::from_bytes(&data)?, data));
        }
        let atlas = TextureAtlas::from_raw_data(&system_palette, texture_headers)?;
        let mut active_frame = None;

        let flaps_down = draw_mode.flaps_down;
        let gear_position = draw_mode.gear_position;
        let bay_position = draw_mode.bay_position;
        let airbrake_extended = draw_mode.airbrake_extended;
        let hook_extended = draw_mode.hook_extended;
        let afterburner_enabled = draw_mode.afterburner_enabled;
        let rudder_position = draw_mode.rudder_position;
        let current_ticks = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();

        let call_names = vec![
            "do_start_interp",
            "_CATGUYDraw@4",
            "@HARDNumLoaded@8",
            "@HardpointAngle@4",
            "_InsectWingAngle@0",
        ];
        let mut interp = i386::Interpreter::new();
        let mut _v = [0u8; 0x100];
        _v[0x8E + 1] = 0x1;
        for tramp in sh.trampolines.iter() {
            if call_names.contains(&tramp.name.as_ref()) {
                interp.add_trampoline(tramp.mem_location, &tramp.name, 1);
                continue;
            }
            println!(
                "Adding port for {} at {:08X}",
                tramp.name, tramp.mem_location
            );
            match tramp.name.as_ref() {
                "_currentTicks" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _currentTicks");
                        current_ticks as u32
                    }),
                ),
                "_lowMemory" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _lowMemory");
                        0
                    }),
                ),
                "_nightHazing" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _nightHazing");
                        1
                    }),
                ),
                "_PLafterBurner" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLafterBurner");
                        if afterburner_enabled {
                            1
                        } else {
                            0
                        }
                    }),
                ),
                "_PLbayOpen" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLbayOpen");
                        if bay_position.is_some() {
                            1
                        } else {
                            0
                        }
                    }),
                ),
                "_PLbayDoorPos" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLbayDoorPosition");
                        if let Some(p) = bay_position {
                            p
                        } else {
                            0
                        }
                    }),
                ),
                "_PLbrake" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLbrake");
                        if airbrake_extended {
                            1
                        } else {
                            0
                        }
                    }),
                ),
                "_PLcanardPos" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLcanardPos");
                        0
                    }),
                ),
                "_PLdead" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLdead");
                        0
                    }),
                ),
                "_PLgearDown" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLgearDown");
                        if gear_position.is_some() {
                            1
                        } else {
                            0
                        }
                    }),
                ),
                "_PLgearPos" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLgearPos");
                        if let Some(p) = gear_position {
                            p
                        } else {
                            0
                        }
                    }),
                ),
                "_PLhook" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLhook");
                        if hook_extended {
                            1
                        } else {
                            0
                        }
                    }),
                ),
                "_PLrightFlap" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLrightFlap");
                        if flaps_down {
                            0xFFFF_FFFF
                        } else {
                            0
                        }
                    }),
                ),
                "_PLleftFlap" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLleftFlap");
                        if flaps_down {
                            0xFFFF_FFFF
                        } else {
                            0
                        }
                    }),
                ),
                "_PLrightAln" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLrightAln");
                        0
                    }),
                ),
                "_PLleftAln" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLleftAln");
                        0
                    }),
                ),
                "_PLrudder" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLrudder");
                        rudder_position as u32
                    }),
                ),
                "_PLslats" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLslats");
                        0
                    }),
                ),
                "_PLstate" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLstate");
                        0
                    }),
                ),
                "_PLswingWing" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLswingWing");
                        0
                    }),
                ),
                "_PLvtAngle" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP: _PLvtAngle");
                        0
                    }),
                ),
                "_PLvtOn" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _PLvtOn");
                        0
                    }),
                ),

                "_SAMcount" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP _SAMcount");
                        4
                    }),
                ),

                "brentObjId" => interp.add_read_port(
                    tramp.mem_location,
                    Box::new(move || {
                        println!("LOOKUP brentObjId");
                        INST_BASE
                    }),
                ),

                "_effectsAllowed" => {
                    interp.add_read_port(
                        tramp.mem_location,
                        Box::new(move || {
                            println!("LOOKUP _effectsAllowed");
                            2
                        }),
                    );
                    interp.add_write_port(
                        tramp.mem_location,
                        Box::new(move |value| {
                            println!("WOULD UPDATE _effectsAllowed: {}", value);
                        }),
                    );
                }
                "_effects" => {
                    interp.add_read_port(
                        tramp.mem_location,
                        Box::new(move || {
                            println!("LOOKUP _effects");
                            2
                        }),
                    );
                    interp.add_write_port(
                        tramp.mem_location,
                        Box::new(move |value| {
                            println!("WOULD UPDATE _effects: {}", value);
                        }),
                    );
                }
                "lighteningAllowed" => interp.add_write_port(
                    tramp.mem_location,
                    Box::new(move |value| {
                        println!("WOULD UPDATE lighteningAllowed: {}", value);
                    }),
                ),
                "mapAdj" => interp.add_write_port(
                    tramp.mem_location,
                    Box::new(move |value| {
                        println!("WOULD UPDATE mapAdj: {}", value);
                    }),
                ),

                "_v" => {
                    interp.map_readonly(tramp.mem_location, &_v).unwrap();
                }

                _ => {}
            }
        }
        for instr in &sh.instrs {
            match instr {
                // Written into by windmill with (_currentTicks & 0xFF) << 2.
                // The frame of animation to show, maybe?
                Instr::XformUnmask(ref c4) => {
                    interp.add_write_port(
                        0xAA00_0000 + c4.offset as u32 + 2,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C4.t0 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c4.offset as u32 + 2 + 2,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C4.t1 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c4.offset as u32 + 2 + 4,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C4.t2 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c4.offset as u32 + 2 + 6,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C4.a0 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c4.offset as u32 + 2 + 8,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C4.a1 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c4.offset as u32 + 2 + 0xA,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C4.a2 <= {:08X}", value);
                        }),
                    );
                }
                Instr::XformUnmask4(ref c6) => {
                    interp.add_write_port(
                        0xAA00_0000 + c6.offset as u32 + 2,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C6.t0 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c6.offset as u32 + 2 + 2,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C6.t1 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c6.offset as u32 + 2 + 4,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C6.t2 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c6.offset as u32 + 2 + 6,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C6.a0 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c6.offset as u32 + 2 + 8,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C6.a1 <= {:08X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + c6.offset as u32 + 2 + 0xA,
                        Box::new(move |value| {
                            println!("WOULD UPDATE C6.a2 <= {:08X}", value);
                            /*
                            if !over.contains_key(&off) {
                                over.insert(off, [0f32; 6]);
                            }
                            if let Some(vs) = c4_overlays.get_mut(&c4.offset) {
                                vs[5] = (value as i32) as f32;
                            }
                            */
                        }),
                    );
                }
                Instr::UnkE4(ref e4) => {
                    let mut v = Vec::new();
                    for i in 0..sh::UnkE4::SIZE {
                        v.push(unsafe { *e4.data.add(i) });
                    }
                    interp
                        .map_writable((0xAA00_0000 + e4.offset) as u32, v)
                        .unwrap();
                }
                Instr::UnkEA(ref ea) => {
                    interp.add_write_port(
                        0xAA00_0000 + ea.offset as u32 + 2,
                        Box::new(move |value| {
                            println!("WOULD UPDATE EA.0 <- {:04X}", value);
                        }),
                    );
                    interp.add_write_port(
                        0xAA00_0000 + ea.offset as u32 + 2 + 2,
                        Box::new(move |value| {
                            println!("WOULD UPDATE EA.2 <- {:04X}", value);
                        }),
                    );
                }
                Instr::UnknownData(ref unk) => {
                    interp
                        .map_writable((0xAA00_0000 + unk.offset) as u32, unk.data.clone())
                        .unwrap();
                }
                Instr::X86Code(ref code) => {
                    interp.add_code(&code.bytecode);
                }
                _ => {}
            }
        }

        // The current pool of vertices.
        let mut vert_pool = Vec::new();

        // We pull from the vert buffer as needed to build faces, because the color and
        // texture information is specified per face.
        let mut indices = Vec::new();
        let mut verts = Vec::new();

        let mut _end_target = None;
        let mut damage_target = None;
        let mut section_close = None;

        let mut unmasked_faces = HashMap::new();
        let mut masking_faces = false;

        let mut byte_offset = 0;
        let mut offset = 0;
        while offset < sh.instrs.len() {
            let instr = &sh.instrs[offset];

            // Handle ranged mode before all others. No guarantee we won't be sidetracked;
            // we may need to split this into a different runloop.
            if let Some([start, end]) = draw_mode.range {
                if byte_offset < start {
                    byte_offset += instr.size();
                    offset += 1;
                    continue;
                }
                if byte_offset >= end {
                    byte_offset += instr.size();
                    offset += 1;
                    continue;
                }
            }

            if offset > stop_at_offset {
                trace!("reached configured stopping point");
                break;
            }

            if let Some(close_offset) = section_close {
                if close_offset == byte_offset {
                    trace!("reached section close; stopping");
                    // FIXME: jump to end_offset
                    break;
                }
            }
            if let Some(damage_offset) = damage_target {
                if damage_offset == byte_offset && !draw_mode.damaged {
                    trace!("reached damage section in non-damage draw mode; stopping");
                    // FIXME: jump to end_offset
                    break;
                }
            }

            println!("At: {:3} => {}", offset, instr.show());
            match instr {
                Instr::Jump(jump) => {
                    byte_offset = jump.target_byte_offset();
                    offset = sh.bytes_to_index(byte_offset)?;
                    continue;
                }
                Instr::JumpToDamage(dam) => {
                    damage_target = Some(dam.damage_byte_offset());
                    if draw_mode.damaged {
                        trace!(
                            "jumping to damaged model at {:04X}",
                            dam.damage_byte_offset()
                        );
                        byte_offset = dam.damage_byte_offset();
                        offset = sh.bytes_to_index(byte_offset)?;
                        continue;
                    }
                }
                Instr::JumpToDetail(detail) => {
                    if draw_mode.detail == detail.level {
                        // If we are drawing in a low detail, jump to the relevant model.
                        trace!(
                            "jumping to low detail model at {:04X}",
                            detail.target_byte_offset()
                        );
                        section_close = None;
                        byte_offset = detail.target_byte_offset();
                        offset = sh.bytes_to_index(byte_offset)?;
                        continue;
                    } else {
                        // If in higher detail we want to not draw this section.
                        trace!("setting section close to {}", detail.target_byte_offset());
                        section_close = Some(detail.target_byte_offset());
                    }
                }
                Instr::JumpToFrame(animation) => {
                    byte_offset = animation.target_for_frame(draw_mode.frame_number);
                    offset = sh.bytes_to_index(byte_offset)?;
                    continue;
                }
                Instr::JumpToLOD(lod) => {
                    if draw_mode.closeness > lod.unk1 as usize {
                        // For high detail, the bytes after the c8 up to the indicated end contain
                        // the high detail model.
                        trace!("setting section close to {}", lod.target_byte_offset());
                        section_close = Some(lod.target_byte_offset());
                    } else {
                        // For low detail, the bytes after the c8 end marker contain the low detail
                        // model. We have no way to know how where the close is, so we have to
                        // monitor and abort to end if we hit the damage section?
                        trace!(
                            "jumping to low detail model at {:04X}",
                            lod.target_byte_offset()
                        );
                        byte_offset = lod.target_byte_offset();
                        offset = sh.bytes_to_index(byte_offset)?;
                        continue;
                    }
                }

                Instr::TextureRef(texture) => {
                    active_frame = Some(&atlas.frames[&texture.filename]);
                }

                Instr::X86Code(code) => {
                    let rv = interp.interpret(code.code_offset(0xAA00_0000u32)).unwrap();
                    match rv {
                        ExitInfo::OutOfInstructions => break,
                        ExitInfo::Trampoline(ref name, ref args) => {
                            println!("Got trampoline return to {} with args {:?}", name, args);
                            // FIXME: handle call and set up return if !do_start_interp
                            byte_offset = (args[0] - 0xAA00_0000u32) as usize;
                            offset = sh.map_interpreter_offset_to_instr_offset(args[0]).unwrap();
                            println!("Resuming at instruction {}", offset);
                            continue;
                        }
                    }
                }

                // Masking
                Instr::Unmask(unk) => {
                    unmasked_faces.insert(unk.target_byte_offset(), [0f32; 6]);
                }
                Instr::Unmask4(unk) => {
                    unmasked_faces.insert(unk.target_byte_offset(), [0f32; 6]);
                }
                Instr::XformUnmask(c4) => {
                    let xform = [
                        f32::from(c4.t0),
                        f32::from(c4.t1),
                        f32::from(c4.t2),
                        f32::from(c4.a0),
                        f32::from(c4.a1),
                        f32::from(c4.a2),
                    ];
                    unmasked_faces.insert(c4.target_byte_offset(), xform);
                }
                Instr::XformUnmask4(c6) => {
                    let xform = [
                        f32::from(c6.t0),
                        f32::from(c6.t1),
                        f32::from(c6.t2),
                        f32::from(c6.a0),
                        f32::from(c6.a1),
                        f32::from(c6.a2),
                    ];
                    unmasked_faces.insert(c6.target_byte_offset(), xform);
                }

                Instr::PtrToObjEnd(end) => {
                    // We do not ever not draw from range; maybe there is some other use of
                    // this target offset that we just don't know yet?
                    _end_target = Some(end.end_byte_offset())
                }
                Instr::EndOfObject(_end) => {
                    break;
                }
                Instr::VertexBuf(buf) => {
                    let xform = if vert_pool.is_empty() {
                        masking_faces = false;
                        [0f32; 6]
                    } else if unmasked_faces.contains_key(&instr.at_offset()) {
                        masking_faces = false;
                        unmasked_faces[&instr.at_offset()]
                    } else {
                        masking_faces = true;
                        [0f32; 6]
                    };
                    let r2 = xform[5] / 256f32;
                    let m = Matrix4::new(
                        r2.cos(),
                        -r2.sin(),
                        0f32,
                        xform[0],
                        r2.sin(),
                        r2.cos(),
                        0f32,
                        -xform[1],
                        0f32,
                        0f32,
                        1f32,
                        xform[2],
                        0f32,
                        0f32,
                        0f32,
                        1f32,
                    );

                    Self::align_vertex_pool(&mut vert_pool, buf.buffer_target_offset());

                    for v in &buf.verts {
                        let v0 =
                            Vector4::new(f32::from(v[0]), f32::from(-v[2]), f32::from(v[1]), 1f32);
                        let v1 = m * v0;
                        vert_pool.push(Vertex {
                            position: [v1[0], v1[1], -v1[2]],
                            color: [0.75f32, 0.5f32, 0f32, 1f32],
                            tex_coord: [0f32, 0f32],
                            flags0: 0,
                            flags1: 0,
                            xform_id: 0,
                        });
                    }
                }
                Instr::Facet(facet) => {
                    if !masking_faces {
                        // Load all vertices in this facet into the vertex upload buffer, copying
                        // in the color and texture coords for each face. Note that the layout is
                        // for triangle fans.
                        let mut v_base = verts.len() as u32;
                        for i in 2..facet.indices.len() {
                            // Given that most facets are very short strips, and
                            // we need to copy the vertices anyway, it's not
                            // *that* must worse to just copy the tris over
                            // instead of trying to get strips or fans working.
                            // TODO: use triangle fans directly
                            let js = [0, i - 1, i];
                            for j in &js {
                                let index = facet.indices[*j] as usize;
                                let tex_coord = if facet.flags.contains(FacetFlags::HAVE_TEXCOORDS)
                                {
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
                                v.color = system_palette.rgba_f32(facet.color as usize)?;
                                if facet.flags.contains(FacetFlags::FILL_BACKGROUND)
                                    || facet.flags.contains(FacetFlags::UNK1)
                                    || facet.flags.contains(FacetFlags::UNK5)
                                {
                                    v.flags0 = 1;
                                }
                                if facet.flags.contains(FacetFlags::HAVE_TEXCOORDS) {
                                    assert!(active_frame.is_some());
                                    let frame = active_frame.unwrap();
                                    v.tex_coord = frame.tex_coord_at(tex_coord);
                                }
                                verts.push(v);
                                indices.push(v_base);
                                v_base += 1;
                            }
                        }
                    }
                }
                _ => {}
            }

            offset += 1;
            byte_offset += instr.size();
        }

        trace!(
            "uploading vertex buffer with {} bytes",
            std::mem::size_of::<Vertex>() * verts.len()
        );
        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

        trace!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        let (texture, tex_future) = Self::upload_texture_rgba(window, atlas.img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        let inst = ShInstance {
            push_constants: vs::ty::PushConstantData::new(),
            pds,
            vertex_buffer,
            index_buffer,
        };

        self.instance = Some(inst);

        Ok(())
    }

    pub fn render(
        &self,
        command_buffer: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let inst = self.instance.clone().unwrap();
        Ok(command_buffer.draw_indexed(
            self.pipeline.clone(),
            dynamic_state,
            vec![inst.vertex_buffer.clone()],
            inst.index_buffer.clone(),
            inst.pds.clone(),
            inst.push_constants,
        )?)
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

#[cfg(test)]
mod test {
    use super::*;
    use failure::Error;
    use omnilib::OmniLib;
    use sh::RawShape;
    use window::GraphicsConfigBuilder;

    #[test]
    fn it_can_render_shapes() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let omni = OmniLib::new_for_test_in_games(&[
            "USNF", "MF", "ATF", "ATFNATO", "ATFGOLD", "USNF97", "FA",
        ])?;
        let skipped = vec![
            "CHAFF.SH",
            "CRATER.SH",
            "DEBRIS.SH",
            "EXP.SH",
            "FIRE.SH",
            "FLARE.SH",
            "MOTHB.SH",
            "SMOKE.SH",
            "WAVE1.SH",
            "WAVE2.SH",
        ];
        let mut palettes = HashMap::new();
        for (game, name) in omni.find_matching("*.SH")?.iter() {
            if skipped.contains(&name.as_ref()) {
                continue;
            }

            println!(
                "At: {}:{:13} @ {}",
                game,
                name,
                omni.path(game, name)
                    .or_else::<Error, _>(|_| Ok("<none>".to_string()))?
            );

            let lib = omni.library(game);
            if !palettes.contains_key(game) {
                let system_palette = Arc::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?);
                palettes.insert(game, system_palette);
            }

            let sh = RawShape::from_bytes(&lib.load(name)?)?;
            let system_palette = palettes[game].clone();
            let mut sh_renderer = ShRenderer::new(&window)?;

            let draw_mode = DrawMode {
                range: None,
                damaged: false,
                closeness: 0x200,
                frame_number: 0,
                detail: 4,
                gear_position: Some(18),
                bay_position: Some(18),
                flaps_down: false,
                airbrake_extended: true,
                hook_extended: true,
                afterburner_enabled: true,
                rudder_position: 0,
            };
            // sh_renderer.legacy_add_shape_to_render(
            //     system_palette.clone(),
            //     &sh,
            //     usize::max_value(),
            //     &draw_mode,
            //     &lib,
            //     &window,
            // )?;

            sh_renderer.add_shape_to_render(&system_palette, &sh, &lib, &window)?;

            window.drive_frame(|command_buffer, dynamic_state| {
                sh_renderer.render(command_buffer, dynamic_state)
            })?;
        }
        std::mem::drop(window);
        Ok(())
    }
}
