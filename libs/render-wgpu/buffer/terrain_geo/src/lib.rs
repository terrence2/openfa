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
use absolute_unit::{Kilometers, Radians};
use camera::ArcBallCamera;
use failure::Fallible;
use frame_graph::CopyBufferDescriptor;
use geodesy::{Cartesian, GeoCenter, Graticule};
use geometry::{algorithm::solid_angle, IcoSphere};
use gpu::GPU;
use memoffset::offset_of;
use nalgebra::Vector3;
use std::{
    cell::RefCell,
    cmp::{Ord, Ordering},
    collections::BinaryHeap,
    mem,
    ops::Range,
    sync::Arc,
};
use wgpu;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct PatchVertex {
    position: [f32; 3],
    normal: [f32; 3],
    graticule: [f32; 2],
}

impl PatchVertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 0,
                    shader_location: 0,
                },
                // normal
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 12,
                    shader_location: 1,
                },
                // graticule
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float2,
                    offset: 24,
                    shader_location: 2,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(PatchVertex, position) as wgpu::BufferAddress
        );

        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(PatchVertex, normal) as wgpu::BufferAddress
        );

        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(PatchVertex, graticule) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<PatchVertex>(), 32);

        tmp
    }
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct Vertex {
    position: [f32; 4],
}

impl Vertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float4,
                    offset: 0,
                    shader_location: 0,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(Vertex, position) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<Vertex>(), 16);

        tmp
    }
}

#[derive(Debug, Copy, Clone)]
struct PatchInfo {
    level: usize,
    solid_angle: f64,
    vertices: [Vector3<f64>; 3],
}

impl PatchInfo {
    fn new(
        level: usize,
        eye_position: &Vector3<f64>,
        eye_direction: &Vector3<f64>,
        vertices: [Vector3<f64>; 3],
    ) -> Self {
        let solid_angle = solid_angle(&eye_position, &eye_direction, &vertices);
        Self {
            level,
            solid_angle,
            vertices,
        }
    }
}

impl Eq for PatchInfo {}

impl PartialEq for PatchInfo {
    fn eq(&self, other: &Self) -> bool {
        self.solid_angle == other.solid_angle
    }
}

impl PartialOrd for PatchInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.solid_angle.partial_cmp(&other.solid_angle)
    }
}
impl Ord for PatchInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Less)
    }
}

pub struct TerrainGeoBuffer {
    // bind_group_layout: wgpu::BindGroupLayout,
    // bind_group: wgpu::BindGroup,
    sphere: IcoSphere,

    num_patches: usize,
    patch_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    patch_index_buffer: wgpu::Buffer,
}

const EARTH_TO_KM: f64 = 6370.0;

impl TerrainGeoBuffer {
    pub fn new(
        num_patches: usize,
        _gen_subdivisions: usize,
        device: &wgpu::Device,
    ) -> Fallible<Arc<RefCell<Self>>> {
        /*
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[wgpu::BindGroupLayoutBinding {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::StorageBuffer {
                    dynamic: false,
                    readonly: true,
                },
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &params_buffer,
                    range: 0..buffer_size,
                },
            }],
        });
        */

        println!(
            "patch_vertex_buffer: {:08X}",
            mem::size_of::<PatchVertex>() * 3 * num_patches
        );
        let patch_vertex_buffer =
            Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
                size: (mem::size_of::<PatchVertex>() * 3 * num_patches) as wgpu::BufferAddress,
                usage: wgpu::BufferUsage::all(),
            })));

        let mut patch_indices = Vec::new();
        patch_indices.push(0u32);
        patch_indices.push(1u32);
        patch_indices.push(1u32);
        patch_indices.push(2u32);
        patch_indices.push(2u32);
        patch_indices.push(0u32);
        let patch_index_buffer = device
            .create_buffer_mapped(patch_indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&patch_indices);

        Ok(Arc::new(RefCell::new(Self {
            sphere: IcoSphere::new(0),

            num_patches,
            patch_vertex_buffer,
            patch_index_buffer,
        })))
    }

    pub fn make_upload_buffer(
        &self,
        camera: &ArcBallCamera,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        let camera_target = camera.cartesian_target_position::<Kilometers>().vec64();
        let eye_position = camera.cartesian_eye_position::<Kilometers>().vec64();
        let eye_direction = camera_target - eye_position;

        let mut patches = BinaryHeap::new();
        for face in &self.sphere.faces {
            // Cull back-facing
            /*
            let angle = face.normal.dot(&eye_direction);
            if angle > 0f64 {
                continue;
            }
            */

            // TODO: Cull below horizon

            // TODO: Cull outside the view frustum

            let v0 = self.sphere.verts[face.i0()] * EARTH_TO_KM;
            let v1 = self.sphere.verts[face.i1()] * EARTH_TO_KM;
            let v2 = self.sphere.verts[face.i2()] * EARTH_TO_KM;
            patches.push(PatchInfo::new(
                0,
                &eye_position,
                &eye_direction,
                [v0, v1, v2],
            ))
        }

        // Split patches until we have an optimal equal-area partitioning.
        while patches.len() < self.num_patches - 4 {
            let patch = patches.pop().unwrap();
            let [v0, v1, v2] = patch.vertices;
            let a = IcoSphere::bisect_edge(&v0, &v1).normalize() * EARTH_TO_KM;
            let b = IcoSphere::bisect_edge(&v1, &v2).normalize() * EARTH_TO_KM;
            let c = IcoSphere::bisect_edge(&v2, &v0).normalize() * EARTH_TO_KM;

            patches.push(PatchInfo::new(
                patch.level + 1,
                &eye_position,
                &eye_direction,
                [v0, a, c],
            ));
            patches.push(PatchInfo::new(
                patch.level + 1,
                &eye_position,
                &eye_direction,
                [v1, b, a],
            ));
            patches.push(PatchInfo::new(
                patch.level + 1,
                &eye_position,
                &eye_direction,
                [v2, c, b],
            ));
            patches.push(PatchInfo::new(
                patch.level + 1,
                &eye_position,
                &eye_direction,
                [a, b, c],
            ));
        }

        let mut verts = Vec::new();
        for patch in &patches {
            let [v0, v1, v2] = patch.vertices;
            let n0 = v0.normalize();
            let n1 = v1.normalize();
            let n2 = v2.normalize();
            verts.push(PatchVertex {
                position: [v0[0] as f32, v0[1] as f32, v0[2] as f32],
                normal: [n0[0] as f32, n0[1] as f32, n0[2] as f32],
                graticule: Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(
                    v0,
                ))
                .lat_lon::<Radians, f32>(),
            });
            verts.push(PatchVertex {
                position: [v1[0] as f32, v1[1] as f32, v1[2] as f32],
                normal: [n1[0] as f32, n1[1] as f32, n1[2] as f32],
                graticule: Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(
                    v1,
                ))
                .lat_lon::<Radians, f32>(),
            });
            verts.push(PatchVertex {
                position: [v2[0] as f32, v2[1] as f32, v2[2] as f32],
                normal: [n2[0] as f32, n2[1] as f32, n2[2] as f32],
                graticule: Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(
                    v2,
                ))
                .lat_lon::<Radians, f32>(),
            });
        }
        let vertex_buffer = gpu
            .device()
            .create_buffer_mapped(verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&verts);
        upload_buffers.push(CopyBufferDescriptor::new(
            vertex_buffer,
            self.patch_vertex_buffer.clone(),
            (mem::size_of::<PatchVertex>() * verts.len()) as wgpu::BufferAddress,
        ));
        Ok(())
    }

    /*
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
    pub fn block_bind_group(&self) -> &wgpu::BindGroup {
        &self.block_bind_group
    }
    */

    pub fn num_patches(&self) -> i32 {
        self.num_patches as i32
    }

    pub fn patch_index_buffer(&self) -> &wgpu::Buffer {
        &self.patch_index_buffer
    }

    pub fn patch_vertex_buffer(&self) -> &wgpu::Buffer {
        &self.patch_vertex_buffer
    }

    pub fn patch_index_range(&self) -> Range<u32> {
        0..6
    }
}
