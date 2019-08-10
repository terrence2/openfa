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
mod draw_state;
mod texture_atlas;

use crate::{
    draw_state::DrawState,
    texture_atlas::{Frame, TextureAtlas},
};
use bitflags::bitflags;
use camera::CameraAbstract;
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
    rc::Rc,
    sync::Arc,
    time::Instant,
};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, CpuBufferPool, DeviceLocalBuffer},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DynamicState},
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

// NOTE: if updating MAX_UNIFORMS, the same constant must be fixed up in the vertex shader.
const MAX_XFORMS: usize = 14;
const UNIFORM_POOL_SIZE: usize = MAX_XFORMS * 16;

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
    fn displacement(self, offset: usize) -> Fallible<Self> {
        VertexFlags::from_bits(self.bits() << offset).ok_or_else(|| {
            err_msg(format!(
                "offset {} from {:?} did not yield a valid vertex flags",
                offset, self
            ))
        })
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

            layout(binding = 1) uniform UniformMatrixArray {
                mat4 xforms[14];
            } uma;

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
                gl_Position = pc.projection * pc.view * uma.xforms[xform_id] * vec4(position, 1.0);
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

    fn set_view(&mut self, mat: &Matrix4<f32>) {
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

    pub fn set_mask(&mut self, mask: u64) {
        self.flag_mask0 = (mask & 0xFFFF_FFFF) as u32;
        self.flag_mask1 = (mask >> 32) as u32;
    }
}

pub struct ShapeErrata {
    no_upper_aileron: bool,
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

// More than meets the eye.
pub struct Transformer {
    xform_id: u32,
    vm: Interpreter,
    code_offset: u32,
    data_offset: u32,
    xform_base: [u8; 12],
    inputs: Vec<TransformInput>,
    xform: Matrix4<f32>,
}

fn fa2r(d: f32) -> f32 {
    d * std::f32::consts::PI / 8192f32
}

pub struct ShapeInstance {
    uniform_upload_pool: Arc<CpuBufferPool<[f32; UNIFORM_POOL_SIZE]>>,
    device_uniform_buffer: Arc<DeviceLocalBuffer<[f32; UNIFORM_POOL_SIZE]>>,
    pds: Arc<dyn DescriptorSet + Send + Sync>,
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    index_buffer: Arc<DeviceLocalBuffer<[u32]>>,
    transformers: Vec<Transformer>,

    // Draw properties based on what's in the shape file.
    errata: ShapeErrata,

    // How should we draw the model?
    draw_state: Rc<RefCell<DrawState>>,
}

impl ShapeInstance {
    fn animate(&mut self, start: &Instant, now: &Instant) -> Fallible<()> {
        for transformer in &mut self.transformers {
            let gear_position = self.draw_state.borrow().gear_position() as u32;
            let bay_position = self.draw_state.borrow().bay_position() as u32;
            let thrust_vectoring = self.draw_state.borrow().thrust_vector_position() as i32 as u32;
            let wing_sweep = i32::from(self.draw_state.borrow().wing_sweep_angle()) as u32;
            let vm = &mut transformer.vm;
            let t = (((*now - *start).as_millis() as u32) >> 4) & 0x0FFF;
            for input in &transformer.inputs {
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
            vm.map_writable(transformer.data_offset, transformer.xform_base.to_vec())?;
            let result = vm.interpret(transformer.code_offset)?;
            let (tramp, _) = result.ok_trampoline()?;
            ensure!(tramp == "do_start_interp", "unexpected interp result");
            let xformed = vm.unmap_writable(transformer.data_offset)?;
            #[allow(clippy::transmute_ptr_to_ptr)]
            let words: &[i16] = unsafe { std::mem::transmute(xformed.as_slice()) };
            let trans = Matrix4::new_translation(&Vector3::new(
                f32::from(words[0]),
                -f32::from(words[1]),
                -f32::from(words[2]),
            ));
            let rots = Matrix4::from_euler_angles(
                -fa2r(f32::from(words[4])),
                -fa2r(f32::from(words[3])),
                -fa2r(f32::from(words[5])),
            );
            transformer.xform = trans * rots;
            for input in &transformer.inputs {
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
        }
        Ok(())
    }

    fn before_render(&self, cb: AutoCommandBufferBuilder) -> Fallible<AutoCommandBufferBuilder> {
        let mut uniforms = [0f32; UNIFORM_POOL_SIZE];
        uniforms[0] = 1f32;
        uniforms[5] = 1f32;
        uniforms[10] = 1f32;
        uniforms[15] = 1f32;
        for transformer in &self.transformers {
            let base = transformer.xform_id as usize * 16;
            for i in 0..16 {
                uniforms[base + i] = transformer.xform[i];
            }
        }
        let new_uniforms_buffer = self.uniform_upload_pool.next(uniforms)?;

        Ok(cb.copy_buffer(new_uniforms_buffer, self.device_uniform_buffer.clone())?)
    }

    fn render(
        &self,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        camera: &CameraAbstract,
        cb: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let transform = Matrix4::new_translation(&Vector3::new(6_378_000.0 + 1000.0, 0.0, 0.0));
        let mut push_consts = vs::ty::PushConstantData::new();
        push_consts.set_projection(&camera.projection_matrix());
        push_consts.set_view(&(camera.view_matrix() * transform));
        push_consts.set_mask(
            self.draw_state
                .borrow()
                .build_mask(self.draw_state.borrow().time_origin(), &self.errata)?,
        );

        Ok(cb.draw_indexed(
            pipeline,
            dynamic_state,
            vec![self.vertex_buffer.clone()],
            self.index_buffer.clone(),
            self.pds.clone(),
            push_consts,
        )?)
    }
}

#[derive(Clone)]
pub struct ShapeInstanceRef {
    value: Arc<RefCell<ShapeInstance>>,
}

impl ShapeInstanceRef {
    pub fn new(instance: ShapeInstance) -> Self {
        ShapeInstanceRef {
            value: Arc::new(RefCell::new(instance)),
        }
    }

    pub fn draw_state(&self) -> Rc<RefCell<DrawState>> {
        self.value.borrow().draw_state.clone()
    }

    pub fn animate(&mut self, start: &Instant, now: &Instant) -> Fallible<()> {
        {
            self.value
                .borrow()
                .draw_state
                .borrow_mut()
                .animate(start, now);
        }
        self.value.borrow_mut().animate(start, now)?;
        Ok(())
    }

    pub fn before_render(
        &self,
        cb: AutoCommandBufferBuilder,
    ) -> Fallible<AutoCommandBufferBuilder> {
        self.value.borrow().before_render(cb)
    }

    fn render(
        &self,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        camera: &CameraAbstract,
        cb: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        self.value
            .borrow()
            .render(pipeline, camera, cb, dynamic_state)
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

pub struct ShRenderer {
    start: Instant,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    instances: HashMap<String, ShapeInstanceRef>,
}

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
            start: Instant::now(),
            pipeline,
            instances: HashMap::new(),
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

        let xform_base = match xform {
            Instr::XformUnmask(ins) => ins.xform_base,
            Instr::XformUnmask4(ins) => ins.xform_base,
            _ => bail!("not an xform instruction"),
        };

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

        transformers.push(Transformer {
            xform_id,
            vm: interp,
            code_offset: x86.code_offset(SHAPE_LOAD_BASE),
            data_offset: SHAPE_LOAD_BASE + xform.at_offset() as u32 + 2u32,
            xform_base,
            inputs,
            xform: Matrix4::identity(),
        });

        Ok(())
    }

    fn draw_model(
        &self,
        name: &str,
        sh: &RawShape,
        palette: &Palette,
        atlas: &TextureAtlas,
        selection: DrawSelection,
        window: &GraphicsWindow,
    ) -> Fallible<ShapeInstance> {
        // Outputs
        let mut verts = Vec::new();
        let mut indices = Vec::new();
        let mut xforms = Vec::new();
        xforms.push(Matrix4::<f32>::identity());

        // State
        let mut prop_man = BufferPropsManager {
            seen_flags: VertexFlags::NONE,
            props: HashMap::new(),
            next_xform_id: 1,
            active_xform_id: 0,
        };
        let mut transformers = Vec::new();
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
                            &mut verts,
                            &mut indices,
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
                    Self::push_facet(
                        facet,
                        &vert_pool,
                        palette,
                        active_frame,
                        None,
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
        let vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>> = DeviceLocalBuffer::array(
            window.device(),
            verts.len(),
            BufferUsage::vertex_buffer_transfer_destination(),
            window.device().active_queue_families(),
        )?;
        let vertex_upload_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

        trace!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let index_buffer: Arc<DeviceLocalBuffer<[u32]>> = DeviceLocalBuffer::array(
            window.device(),
            indices.len(),
            BufferUsage::index_buffer_transfer_destination(),
            window.device().active_queue_families(),
        )?;
        let index_upload_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        let cb = AutoCommandBufferBuilder::primary_one_time_submit(
            window.device(),
            window.queue().family(),
        )?
        .copy_buffer(vertex_upload_buffer.clone(), vertex_buffer.clone())?
        .copy_buffer(index_upload_buffer.clone(), index_buffer.clone())?
        .build()?;
        let upload_future = cb.execute(window.queue())?;

        let (texture, tex_future) = Self::upload_texture_rgba(window, atlas.img.to_rgba())?;
        let sampler = Self::make_sampler(window.device())?;

        // FIXME: lift this up to the top level
        upload_future
            .join(tex_future)
            .then_signal_fence_and_flush()?
            .cleanup_finished();

        let device_uniform_buffer: Arc<DeviceLocalBuffer<[f32; UNIFORM_POOL_SIZE]>> =
            DeviceLocalBuffer::new(
                window.device(),
                BufferUsage::uniform_buffer_transfer_destination(),
                window.device().active_queue_families(),
            )?;

        let uniform_upload_pool = Arc::new(CpuBufferPool::upload(window.device()));

        let pds = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .add_buffer(device_uniform_buffer.clone())?
                .build()?,
        );

        Ok(ShapeInstance {
            uniform_upload_pool,
            device_uniform_buffer,
            pds,
            vertex_buffer,
            index_buffer,
            transformers,
            errata: ShapeErrata::from_flags(prop_man.seen_flags),
            draw_state: Default::default(),
        })
    }

    pub fn add_shape_to_render(
        &mut self,
        name: &str,
        sh: &RawShape,
        selection: DrawSelection,
        palette: &Palette,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<ShapeInstanceRef> {
        if self.instances.contains_key(name) {
            return Ok(self.instances[name].clone());
        }

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
        let instance = self.draw_model(name, sh, palette, &atlas, selection, window)?;

        let instance_ref = ShapeInstanceRef::new(instance);

        self.instances.insert(name.to_owned(), instance_ref.clone());
        Ok(instance_ref)
    }

    pub fn animate(&mut self, now: &Instant) -> Fallible<()> {
        for instance in self.instances.values_mut() {
            instance.animate(&self.start, now)?;
        }
        Ok(())
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

    pub fn before_render(
        &self,
        command_buffer: AutoCommandBufferBuilder,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let mut cb = command_buffer;
        for instance in self.instances.values() {
            cb = instance.before_render(cb)?;
        }
        Ok(cb)
    }

    pub fn render(
        &self,
        camera: &CameraAbstract,
        command_buffer: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let mut cb = command_buffer;
        for instance in self.instances.values() {
            cb = instance.render(self.pipeline.clone(), camera, cb, dynamic_state)?;
        }
        Ok(cb)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use camera::ArcBallCamera;
    use failure::Error;
    use omnilib::OmniLib;
    use sh::RawShape;
    use std::{f64::consts::PI, rc::Rc};
    use window::GraphicsConfigBuilder;

    #[test]
    fn it_can_render_shapes() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
        camera.set_distance(100.);
        camera.set_angle(115. * PI / 180., -135. * PI / 180.);

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
        for (game, lib) in omni.libraries() {
            let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));

            for name in &lib.find_matching("*.SH")? {
                if skipped.contains(&name.as_ref()) {
                    continue;
                }

                println!(
                    "At: {}:{:13} @ {}",
                    game,
                    name,
                    omni.path(&game, name)
                        .or_else::<Error, _>(|_| Ok("<none>".to_string()))?
                );

                let sh = RawShape::from_bytes(&lib.load(name)?)?;
                let mut sh_renderer = ShRenderer::new(&window)?;

                let sh_instance = sh_renderer.add_shape_to_render(
                    &(game.to_owned() + ":" + name),
                    &sh,
                    DrawSelection::NormalModel,
                    &system_palette,
                    &lib,
                    &window,
                )?;
                sh_instance
                    .draw_state()
                    .borrow_mut()
                    .toggle_gear(&Instant::now());
                sh_renderer.animate(&Instant::now())?;

                {
                    let frame = window.begin_frame()?;
                    if !frame.is_valid() {
                        continue;
                    }

                    let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                        window.device(),
                        window.queue().family(),
                    )?;

                    cbb = sh_renderer.before_render(cbb)?;

                    cbb = cbb.begin_render_pass(
                        frame.framebuffer(&window),
                        false,
                        vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
                    )?;

                    cbb = sh_renderer.render(&camera, cbb, &window.dynamic_state)?;

                    cbb = cbb.end_render_pass()?;

                    let cb = cbb.build()?;

                    frame.submit(cb, &mut window)?;
                }
            }
        }
        std::mem::drop(window);
        Ok(())
    }
}
