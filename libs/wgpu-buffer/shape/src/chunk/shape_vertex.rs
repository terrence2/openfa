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
use crate::chunk::upload::BufferProps;
use absolute_unit::{Feet, Length};
use anyhow::{anyhow, Result};
use bitflags::bitflags;
use memoffset::offset_of;
use nalgebra::{Point3, Vector3};
use std::mem;
use zerocopy::{AsBytes, FromBytes};

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

        const IS_VERTEX_NORMAL     = 0x0100_0000_0000_0000;

        const AILERONS_DOWN        = Self::LEFT_AILERON_DOWN.bits | Self::RIGHT_AILERON_DOWN.bits;
        const AILERONS_UP          = Self::LEFT_AILERON_UP.bits | Self::RIGHT_AILERON_UP.bits;
    }
}

impl VertexFlags {
    pub fn displacement(self, offset: usize) -> Result<Self> {
        VertexFlags::from_bits(self.bits() << offset).ok_or_else(|| {
            anyhow!(format!(
                "offset {offset} from {self:?} did not yield a valid vertex flags"
            ))
        })
    }
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
pub struct ShapeVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    tex_coord: [u32; 2], // pixel offset
    flags0: u32,
    flags1: u32,
    xform_id: u32,
}

impl ShapeVertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        let tmp = wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                // normal
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                // color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                // tex_coord
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x2,
                    offset: 40,
                    shader_location: 3,
                },
                // flags0
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 48,
                    shader_location: 4,
                },
                // flags1
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 52,
                    shader_location: 5,
                },
                // xform_id
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 56,
                    shader_location: 6,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(ShapeVertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(ShapeVertex, normal) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(ShapeVertex, color) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[3].offset,
            offset_of!(ShapeVertex, tex_coord) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[4].offset,
            offset_of!(ShapeVertex, flags0) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[5].offset,
            offset_of!(ShapeVertex, flags1) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[6].offset,
            offset_of!(ShapeVertex, xform_id) as wgpu::BufferAddress
        );

        tmp
    }

    pub(crate) fn new(position: &Point3<Length<Feet>>, props: &BufferProps) -> Self {
        Self {
            // Color and Tex Coords will be filled out by the
            // face when we move this into the verts list.
            color: [0.75f32, 0.5f32, 0f32, 1f32],
            tex_coord: [0u32; 2],
            // Normal may be a vertex normal or face normal, depending.
            // It will also be filled out as we discover more.
            normal: [0f32; 3],
            // Base position, flags, and the xform are constant
            // for this entire buffer, independent of the face.
            position: [position.x.f32(), position.y.f32(), position.z.f32()],
            flags0: (props.flags.bits() & 0xFFFF_FFFF) as u32,
            flags1: (props.flags.bits() >> 32) as u32,
            xform_id: props.xform_id,
        }
    }

    pub fn overlay_slice(buf: &[u8]) -> Result<&[Self]> {
        zerocopy::LayoutVerified::<&[u8], [Self]>::new_slice(buf)
            .map(|v| v.into_slice())
            .ok_or_else(|| anyhow!("cannot overlay slice"))
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        self.color = color;
    }

    pub fn set_raw_tex_coords(&mut self, s: u32, t: u32) {
        self.tex_coord = [s, t];
    }

    pub fn flags(&self) -> VertexFlags {
        VertexFlags::from_bits(self.flags0 as u64 | (self.flags1 as u64) << 32).unwrap()
    }

    pub fn set_flags(&mut self, flags: VertexFlags) {
        self.flags0 = (flags.bits() & 0xFFFF_FFFF) as u32;
        self.flags1 = (flags.bits() >> 32) as u32;
    }

    pub fn set_is_blend_texture(&mut self) {
        self.flags0 |= (VertexFlags::BLEND_TEXTURE.bits() & 0xFFFF_FFFF) as u32;
    }

    pub fn set_is_vertex_normal(&mut self) {
        self.flags1 |= (VertexFlags::IS_VERTEX_NORMAL.bits() >> 32) as u32;
    }

    pub fn position(&self) -> &[f32; 3] {
        &self.position
    }

    pub fn point(&self) -> Point3<f32> {
        Point3::new(self.position[0], self.position[2], -self.position[1])
    }

    pub fn normal(&self) -> Vector3<f32> {
        Vector3::new(self.normal[0], self.normal[2], -self.normal[1])
    }

    pub fn set_normal(&mut self, normal: [f32; 3]) {
        self.normal = normal;
    }

    pub fn is_vertex_normal(&self) -> bool {
        self.flags1 & (VertexFlags::IS_VERTEX_NORMAL.bits() >> 32) as u32 != 0
    }
}

impl Default for ShapeVertex {
    fn default() -> Self {
        Self {
            position: [0f32; 3],
            normal: [0f32; 3],
            color: [0.75f32, 0.5f32, 0f32, 1f32],
            tex_coord: [0u32; 2],
            flags0: 0,
            flags1: 0,
            xform_id: 0,
        }
    }
}
