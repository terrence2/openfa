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

const ANIMATION_FRAME_TIME: usize = 166; // ms

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
        const SLATS_DOWN           = 0x0000_0000_0200_0000;
        const SLATS_UP             = 0x0000_0000_0400_0000;
        const BAY_OPEN             = 0x0000_0000_0800_0000;
        const BAY_CLOSED           = 0x0000_0000_1000_0000;

        const PLAYER_ALIVE         = 0x0000_0000_2000_0000;
        const PLAYER_DEAD          = 0x0000_0000_4000_0000;

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

        const EJECT_STATE_0        = 0x0000_0400_0000_0000;
        const EJECT_STATE_1        = 0x0000_0800_0000_0000;
        const EJECT_STATE_2        = 0x0000_1000_0000_0000;
        const EJECT_STATE_3        = 0x0000_2000_0000_0000;
        const EJECT_STATE_4        = 0x0000_4000_0000_0000;


        const AILERONS_DOWN        = Self::LEFT_AILERON_DOWN.bits | Self::RIGHT_AILERON_DOWN.bits;
        const AILERONS_UP          = Self::LEFT_AILERON_UP.bits | Self::RIGHT_AILERON_UP.bits;
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
    frame_count: usize,
    no_upper_aileron: bool,
    // has_toggle_gear: bool,
    // has_toggle_bay: bool,
}

impl ShapeErrata {
    fn new(frame_count: usize, flags: VertexFlags) -> Self {
        Self {
            frame_count,

            // ERRATA: ATFNATO:F22.SH is missing aileron up meshes.
            no_upper_aileron: !(flags & VertexFlags::AILERONS_DOWN).is_empty()
                && (flags & VertexFlags::AILERONS_UP).is_empty(),
        }
    }
}

pub struct ShapeModel {
    pds: Arc<dyn DescriptorSet + Send + Sync>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,

    // What kind of model was draw into the above buffers.
    selection: DrawSelection,

    // Draw properties based on what's in the shape file.
    errata: ShapeErrata,
}

struct DrawState {
    pub show_damaged: bool,
    //pub frame_number: usize,
    pub gear_position: Option<u32>,
    pub bay_position: Option<u32>,
    pub flaps_down: bool,
    pub slats_down: bool,
    pub airbrake_extended: bool,
    pub hook_extended: bool,
    pub afterburner_enabled: bool,
    pub rudder_position: i32,
    pub left_aileron_position: i32,
    pub right_aileron_position: i32,
    pub sam_count: u32,
    pub eject_state: u32,
    pub player_dead: bool,
}

impl Default for DrawState {
    fn default() -> Self {
        DrawState {
            show_damaged: false,
            //frame_number: 0,
            gear_position: Some(18),
            flaps_down: false,
            slats_down: false,
            airbrake_extended: true,
            hook_extended: true,
            bay_position: Some(18),
            afterburner_enabled: true,
            rudder_position: 0,
            left_aileron_position: 0,
            right_aileron_position: 0,
            sam_count: 3,
            eject_state: 0,
            player_dead: false,
        }
    }
}

impl DrawState {
    fn build_mask(&self, start: &Instant, errata: &ShapeErrata) -> Fallible<u64> {
        let mut mask = VertexFlags::STATIC | VertexFlags::BLEND_TEXTURE;

        if errata.frame_count > 0 {
            let offset = ((start.elapsed().as_millis() as usize) / ANIMATION_FRAME_TIME)
                % errata.frame_count;
            mask |= VertexFlags::from_bits(VertexFlags::ANIM_FRAME_0.bits() << offset).unwrap();
        }

        mask |= if self.flaps_down {
            VertexFlags::LEFT_FLAP_DOWN | VertexFlags::RIGHT_FLAP_DOWN
        } else {
            VertexFlags::LEFT_FLAP_UP | VertexFlags::RIGHT_FLAP_UP
        };

        mask |= if self.slats_down {
            VertexFlags::SLATS_DOWN
        } else {
            VertexFlags::SLATS_UP
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

        mask |= if self.left_aileron_position < 0 {
            VertexFlags::LEFT_AILERON_DOWN
        } else if self.left_aileron_position > 0 {
            if errata.no_upper_aileron {
                VertexFlags::LEFT_AILERON_CENTER
            } else {
                VertexFlags::LEFT_AILERON_UP
            }
        } else {
            VertexFlags::LEFT_AILERON_CENTER
        };

        mask |= if self.right_aileron_position < 0 {
            VertexFlags::RIGHT_AILERON_DOWN
        } else if self.right_aileron_position > 0 {
            if errata.no_upper_aileron {
                VertexFlags::RIGHT_AILERON_CENTER
            } else {
                VertexFlags::RIGHT_AILERON_UP
            }
        } else {
            VertexFlags::RIGHT_AILERON_CENTER
        };

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

        mask |= if self.bay_position.is_some() {
            VertexFlags::BAY_OPEN
        } else {
            VertexFlags::BAY_CLOSED
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

        mask |= match self.eject_state {
            0 => VertexFlags::EJECT_STATE_0,
            1 => VertexFlags::EJECT_STATE_1,
            2 => VertexFlags::EJECT_STATE_2,
            3 => VertexFlags::EJECT_STATE_3,
            4 => VertexFlags::EJECT_STATE_4,
            _ => bail!("expected eject state in 0..4"),
        };

        mask |= if self.player_dead {
            VertexFlags::PLAYER_DEAD
        } else {
            VertexFlags::PLAYER_ALIVE
        };

        Ok(mask.bits())
    }
}

pub struct ShapeInstance {
    models: Vec<Arc<ShapeModel>>,
    draw_state: DrawState,
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

    pub fn build_render_mask(&self, start: &Instant, errata: &ShapeErrata) -> Fallible<u64> {
        self.value
            .try_borrow()?
            .draw_state
            .build_mask(start, errata)
    }

    pub fn get_models(&self) -> Vec<Arc<ShapeModel>> {
        self.value.as_ref().borrow().models.clone()
    }

    pub fn toggle_flaps(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.flaps_down = !ds.flaps_down;
        Ok(())
    }

    pub fn has_flaps_down(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.flaps_down)
    }

    pub fn toggle_slats(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.slats_down = !ds.slats_down;
        Ok(())
    }

    pub fn has_slats_down(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.slats_down)
    }

    pub fn toggle_hook(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.hook_extended = !ds.hook_extended;
        Ok(())
    }

    pub fn has_hook_extended(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.hook_extended)
    }

    pub fn toggle_airbrake(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.airbrake_extended = !ds.airbrake_extended;
        Ok(())
    }

    pub fn has_airbrake_extended(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.airbrake_extended)
    }

    pub fn enable_afterburner(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.afterburner_enabled = true;
        Ok(())
    }

    pub fn disable_afterburner(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.afterburner_enabled = false;
        Ok(())
    }

    pub fn has_afterburner_enabled(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.afterburner_enabled)
    }

    pub fn toggle_bay(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        // FIXME: implement non-toggle bay doors
        ds.bay_position = if ds.bay_position.is_some() {
            None
        } else {
            Some(0)
        };
        Ok(())
    }

    pub fn has_bay_open(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.bay_position.is_some())
    }

    pub fn toggle_gear(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        // FIXME: implement non-toggle gear
        ds.gear_position = if ds.gear_position.is_some() {
            None
        } else {
            Some(0)
        };
        Ok(())
    }

    pub fn has_gear_down(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.gear_position.is_some())
    }

    pub fn move_rudder_left(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.rudder_position = -1;
        Ok(())
    }

    pub fn move_rudder_right(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.rudder_position = 1;
        Ok(())
    }

    pub fn move_rudder_center(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.rudder_position = 0;
        Ok(())
    }

    pub fn get_rudder_position(&self) -> Fallible<i32> {
        Ok(self.value.try_borrow()?.draw_state.rudder_position)
    }

    pub fn move_stick_left(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.left_aileron_position = 1;
        ds.right_aileron_position = -1;
        Ok(())
    }

    pub fn move_stick_right(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.left_aileron_position = -1;
        ds.right_aileron_position = 1;
        Ok(())
    }

    pub fn move_stick_center(&mut self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.left_aileron_position = 0;
        ds.right_aileron_position = 0;
        Ok(())
    }

    pub fn get_left_aileron_position(&self) -> Fallible<i32> {
        Ok(self.value.try_borrow()?.draw_state.left_aileron_position)
    }

    pub fn get_right_aileron_position(&self) -> Fallible<i32> {
        Ok(self.value.try_borrow()?.draw_state.right_aileron_position)
    }

    pub fn show_damaged(&self) -> Fallible<bool> {
        Ok(self.value.try_borrow()?.draw_state.show_damaged)
    }

    pub fn toggle_damaged(&self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.show_damaged = !ds.show_damaged;
        Ok(())
    }

    pub fn toggle_player_dead(&self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.player_dead = !ds.player_dead;
        Ok(())
    }

    pub fn bump_eject_state(&self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.eject_state += 1;
        ds.eject_state %= 5;
        Ok(())
    }

    pub fn bump_sam_count(&self) -> Fallible<()> {
        let ds = &mut self.value.try_borrow_mut()?.draw_state;
        ds.sam_count += 1;
        ds.sam_count %= 4;
        Ok(())
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
enum DrawSelection {
    DamageModel,
    NormalModel,
}

impl DrawSelection {
    fn is_damage(&self) -> bool {
        self == &DrawSelection::DamageModel
    }
}

#[derive(Clone, Copy)]
struct BufferProperties {
    flags: VertexFlags,
    xform_id: u32,
}

impl BufferProperties {
    pub fn add(
        tgt: usize,
        flags: VertexFlags,
        seen_flags: &mut VertexFlags,
        props: &mut HashMap<usize, BufferProperties>,
    ) {
        let entry = props.entry(tgt).or_insert(BufferProperties {
            flags: VertexFlags::NONE,
            xform_id: 0,
        });
        entry.flags |= flags;
        *seen_flags |= flags;
    }
}

pub struct ShRenderer {
    start: Instant,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    instances: Vec<ShapeInstanceRef>,
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
            instances: Vec::new(),
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

    fn find_external_calls<'a>(
        x86: &X86Code,
        sh: &'a RawShape,
    ) -> Fallible<HashMap<&'a str, &'a X86Trampoline>> {
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
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        seen_flags: &mut VertexFlags,
        props: &mut HashMap<usize, BufferProperties>,
    ) -> Fallible<usize> {
        let memrefs = Self::find_external_references(x86, sh);
        let next_instr = pc.relative_instr(1, sh);
        if next_instr.magic() == "Unmask" {
            ensure!(
                memrefs.len() == 1,
                "expected unmask with only one parameter"
            );
            let (&name, trampoline) = memrefs.iter().next().expect("checked next");
            if TOGGLE_TABLE.contains_key(name) {
                Self::update_buffer_properties_for_toggle(
                    trampoline, pc, x86, sh, seen_flags, props,
                )?;
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
                Self::update_buffer_properties_for_num_loaded(
                    trampoline, pc, x86, sh, seen_flags, props,
                )?;
            } else {
                bail!("unknown memory read: {}", name)
            }
        }
        Ok(0)
    }

    fn update_buffer_properties_for_toggle(
        trampoline: &X86Trampoline,
        pc: &ProgramCounter,
        x86: &X86Code,
        sh: &RawShape,
        seen_flags: &mut VertexFlags,
        props: &mut HashMap<usize, BufferProperties>,
    ) -> Fallible<()> {
        let unmask = pc.relative_instr(1, sh);
        let trailer = pc.relative_instr(2, sh);
        ensure!(unmask.magic() == "Unmask", "expected unmask after flag x86");
        ensure!(trailer.magic() == "F0", "expected code after unmask");

        let mut interp = i386::Interpreter::new();
        interp.add_code(&x86.bytecode);
        interp.add_code(&trailer.unwrap_x86()?.bytecode);
        let do_start_interp = sh.lookup_trampoline_by_name("do_start_interp")?;
        interp.add_trampoline(do_start_interp.mem_location, &do_start_interp.name, 1);

        for &(value, flags) in &TOGGLE_TABLE[trampoline.name.as_str()] {
            interp.add_read_port(trampoline.mem_location, Box::new(move || value));
            let exit_info = interp.interpret(x86.code_offset(0xAA00_0000u32))?;
            let (name, args) = exit_info.ok_trampoline()?;
            ensure!(name == "do_start_interp", "unexpected trampoline return");
            ensure!(args.len() == 1, "unexpected arg count");
            if unmask.at_offset() == args[0].wrapping_sub(SHAPE_LOAD_BASE) as usize {
                BufferProperties::add(unmask.unwrap_unmask_target()?, flags, seen_flags, props);
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
        seen_flags: &mut VertexFlags,
        props: &mut HashMap<usize, BufferProperties>,
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
        interp.add_code(&x86.bytecode);
        interp.add_code(&trailer.unwrap_x86()?.bytecode);
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
                BufferProperties::add(unmask.unwrap_unmask_target()?, flags, seen_flags, props);
            }
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
    ) -> Fallible<ShapeModel> {
        // Outputs
        let mut frame_count = 0;
        let mut verts = Vec::new();
        let mut indices = Vec::new();
        let mut xforms = Vec::new();
        xforms.push(Matrix4::<f32>::identity());

        // State
        let mut active_frame = None;
        let _active_xform_id = 0;
        let mut seen_flags = VertexFlags::NONE;
        let mut buffer_properties = HashMap::new();
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
                            VertexFlags::from_bits(VertexFlags::ANIM_FRAME_0.bits() << i),
                            &mut verts,
                            &mut indices,
                        )?;
                    }
                    if frame_count != 0 {
                        ensure!(
                            frame_count == frame.num_frames(),
                            "animations with different frame counts",
                        );
                    }
                    frame_count = frame.num_frames();
                    pc.set_byte_offset(frame.target_for_frame(0), sh)?;
                }

                Instr::X86Code(ref x86) => {
                    let advance_cnt = Self::maybe_update_buffer_properties(
                        &pc,
                        x86,
                        sh,
                        &mut seen_flags,
                        &mut buffer_properties,
                    )?;
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

        Ok(ShapeModel {
            pds,
            vertex_buffer,
            index_buffer,
            selection,
            errata: ShapeErrata::new(frame_count, seen_flags),
        })
    }

    pub fn add_shape_to_render(
        &mut self,
        palette: &Palette,
        sh: &RawShape,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<ShapeInstanceRef> {
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

        let mut models = vec![Arc::new(self.draw_model(
            sh,
            palette,
            &atlas,
            DrawSelection::NormalModel,
            window,
        )?)];
        if let Ok(damage_model) =
            self.draw_model(sh, palette, &atlas, DrawSelection::DamageModel, window)
        {
            models.push(Arc::new(damage_model));
        } else {
            // FIXME: load all damage models _{A,B,C,D}
        }

        let instance = ShapeInstanceRef::new(ShapeInstance {
            models,
            draw_state: Default::default(),
        });

        self.instances.push(instance.clone());
        Ok(instance)
    }

    pub fn render(
        &self,
        projection: &Matrix4<f32>,
        view: &Matrix4<f32>,
        command_buffer: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let mut cb = command_buffer;
        for inst in &self.instances {
            for model in inst.get_models() {
                if inst.show_damaged()? != model.selection.is_damage() {
                    continue;
                }
                let mut push_consts = vs::ty::PushConstantData::new();
                push_consts.set_projection(projection);
                push_consts.set_view(view);
                push_consts.set_mask(inst.build_render_mask(&self.start, &model.errata)?);

                cb = cb.draw_indexed(
                    self.pipeline.clone(),
                    dynamic_state,
                    vec![model.vertex_buffer.clone()],
                    model.index_buffer.clone(),
                    model.pds.clone(),
                    push_consts,
                )?;
            }
        }
        Ok(cb)
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
    use crate::ArcBallCamera;
    use failure::Error;
    use omnilib::OmniLib;
    use sh::RawShape;
    use std::{f32::consts::PI, rc::Rc};
    use window::GraphicsConfigBuilder;

    #[test]
    fn it_can_render_shapes() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.1f32, 3.4e+38f32);
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
                let system_palette =
                    Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));
                palettes.insert(game, system_palette);
            }

            let sh = RawShape::from_bytes(&lib.load(name)?)?;
            let system_palette = palettes[game].clone();
            let mut sh_renderer = ShRenderer::new(&window)?;
            let mut sh_instance =
                sh_renderer.add_shape_to_render(&system_palette, &sh, &lib, &window)?;
            sh_instance.toggle_flaps()?;

            window.drive_frame(|command_buffer, dynamic_state| {
                sh_renderer.render(
                    camera.projection_matrix(),
                    &camera.view_matrix(),
                    command_buffer,
                    dynamic_state,
                )
            })?;
        }
        std::mem::drop(window);
        Ok(())
    }
}
