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
mod debug_vertex;
mod icosahedron;
mod patch;
mod patch_tree;
mod patch_vertex;
mod patch_winding;
mod queue;

use crate::patch_tree::PatchTree;
pub use crate::{debug_vertex::DebugVertex, patch_vertex::PatchVertex};

use absolute_unit::Kilometers;
use camera::Camera;
use failure::Fallible;
use frame_graph::CopyBufferDescriptor;
use gpu::GPU;
use nalgebra::{Matrix4, Point3};
use std::{cell::RefCell, mem, ops::Range, sync::Arc};

const DBG_VERT_COUNT: usize = 4096;

const DBG_COLORS_BY_LEVEL: [[f32; 3]; 19] = [
    [0.75, 0.25, 0.25],
    [0.25, 0.75, 0.75],
    [0.75, 0.42, 0.25],
    [0.25, 0.58, 0.75],
    [0.75, 0.58, 0.25],
    [0.25, 0.42, 0.75],
    [0.75, 0.75, 0.25],
    [0.25, 0.25, 0.75],
    [0.58, 0.75, 0.25],
    [0.42, 0.25, 0.75],
    [0.58, 0.25, 0.75],
    [0.42, 0.75, 0.25],
    [0.25, 0.75, 0.25],
    [0.75, 0.25, 0.75],
    [0.25, 0.75, 0.42],
    [0.75, 0.25, 0.58],
    [0.25, 0.75, 0.58],
    [0.75, 0.25, 0.42],
    [0.10, 0.75, 0.72],
];

pub enum CpuDetailLevel {
    Low,
    Medium,
}

impl CpuDetailLevel {
    // max-level, target-refinement, buffer-size
    fn parameters(&self) -> (usize, f64, usize) {
        match self {
            Self::Low => (8, 150.0, 256),
            Self::Medium => (14, 150.0, 768),
        }
    }
}

pub enum GpuDetailLevel {
    Low,
    Medium,
    High,
}

impl GpuDetailLevel {
    // subdivisions
    fn parameters(&self) -> usize {
        match self {
            Self::Low => 3,
            Self::Medium => 5,
            Self::High => 7,
        }
    }

    fn vertices_per_subdivision(d: usize) -> usize {
        (((2f64.powf(d as f64) + 1.0) * (2f64.powf(d as f64) + 2.0)) / 2.0).floor() as usize
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct PatchMetdata {
    vertex_offset: u32,
    index_buffer: u32,
}

pub struct TerrainGeoBuffer {
    // Maximum number of patches for the patch buffer.
    patch_buffer_size: usize,

    patch_tree: PatchTree,

    // bind_group_layout: wgpu::BindGroupLayout,
    // bind_group: wgpu::BindGroup,
    patch_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    patch_index_buffer: wgpu::Buffer,

    dbg_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    dbg_index_buffer: Arc<Box<wgpu::Buffer>>,
    dbg_vertex_count: u32,
}

impl TerrainGeoBuffer {
    pub fn new(
        cpu_detail_level: CpuDetailLevel,
        gpu_detail_level: GpuDetailLevel,
        gpu: &GPU,
    ) -> Fallible<Arc<RefCell<Self>>> {
        let (max_level, target_refinement, patch_buffer_size) = cpu_detail_level.parameters();
        let subdivisions = gpu_detail_level.parameters();

        let patch_tree = PatchTree::new(max_level, target_refinement, patch_buffer_size);

        println!(
            "dbg_vertex_buffer: {:08X}",
            mem::size_of::<DebugVertex>() * DBG_VERT_COUNT
        );
        let dbg_vertex_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("terrain-geo-debug-vertices"),
                size: (mem::size_of::<DebugVertex>() * DBG_VERT_COUNT) as wgpu::BufferAddress,
                usage: wgpu::BufferUsage::all(),
            },
        )));
        let mut dbg_indices = Vec::new();
        dbg_indices.push(0);
        for i in 1u32..DBG_VERT_COUNT as u32 {
            dbg_indices.push(i);
            dbg_indices.push(i);
            dbg_indices.push(i);
        }
        let dbg_index_buffer = Arc::new(Box::new(gpu.push_slice(
            "terrain-geo-debug-indices",
            &dbg_indices,
            wgpu::BufferUsage::all(),
        )));

        println!(
            "patch_vertex_buffer: {:08X}",
            PatchVertex::mem_size() * 3 * patch_buffer_size
        );
        let patch_vertex_buffer_size =
            (PatchVertex::mem_size() * 3 * patch_buffer_size) as wgpu::BufferAddress;
        let patch_vertex_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("terrain-geo-patch-vertex-buffer"),
                size: patch_vertex_buffer_size,
                // TODO: remove vertex usage
                usage: wgpu::BufferUsage::STORAGE_READ
                    | wgpu::BufferUsage::COPY_DST
                    | wgpu::BufferUsage::VERTEX,
            },
        )));

        let mut patch_indices = Vec::new();
        patch_indices.push(0u32);
        patch_indices.push(1u32);
        patch_indices.push(1u32);
        patch_indices.push(2u32);
        patch_indices.push(2u32);
        patch_indices.push(0u32);
        let patch_index_buffer = gpu.push_slice(
            "terrain-geo-patch-indices",
            &patch_indices,
            wgpu::BufferUsage::INDEX,
        );

        // Create target vertex buffer.
        let target_vertex_buffer_size = (mem::size_of::<PatchVertex>()
            * 3
            * patch_buffer_size
            * GpuDetailLevel::vertices_per_subdivision(subdivisions))
            as wgpu::BufferAddress;
        let target_vertex_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("terrain-geo-sub-vertex-buffer"),
                size: target_vertex_buffer_size,
                usage: wgpu::BufferUsage::STORAGE
                    | wgpu::BufferUsage::COPY_DST
                    | wgpu::BufferUsage::VERTEX,
            },
        )));

        let subdivide_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-subdivide-bind-group-layout"),
                    bindings: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: true,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::COMPUTE,
                            ty: wgpu::BindingType::StorageBuffer {
                                dynamic: false,
                                readonly: false,
                            },
                        },
                    ],
                });
        let subdivide_shader =
            gpu.create_shader_module(include_bytes!("../target/subdivide.comp.spirv"))?;
        let subdivide_pipeline =
            gpu.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    layout: &gpu
                        .device()
                        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                            bind_group_layouts: &[&subdivide_bind_group_layout],
                        }),
                    compute_stage: wgpu::ProgrammableStageDescriptor {
                        module: &subdivide_shader,
                        entry_point: "main",
                    },
                });

        let subdivide_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-geo-subdivide-bind-group"),
            layout: &subdivide_bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &patch_vertex_buffer,
                        range: 0..patch_vertex_buffer_size,
                    },
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &target_vertex_buffer,
                        range: 0..target_vertex_buffer_size,
                    },
                },
            ],
        });

        Ok(Arc::new(RefCell::new(Self {
            patch_buffer_size,
            patch_tree,

            patch_vertex_buffer,
            patch_index_buffer,

            dbg_vertex_buffer,
            dbg_index_buffer,
            dbg_vertex_count: 0,
        })))
    }

    pub fn make_upload_buffer(
        &mut self,
        camera: &Camera,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        let mut dbg_verts = Vec::with_capacity(3 * self.patch_buffer_size);
        let mut verts = Vec::with_capacity(3 * self.patch_buffer_size);
        let mut dbg_indices = Vec::with_capacity(3 * self.patch_buffer_size);
        let mut live_patches = Vec::with_capacity(self.patch_buffer_size);
        self.patch_tree.optimize_for_view(camera, &mut live_patches);
        assert!(live_patches.len() <= self.patch_buffer_size);

        let scale = Matrix4::new_scaling(1_000.0);
        let view = camera.view::<Kilometers>();
        for (offset, (i, _winding)) in live_patches.iter().enumerate() {
            let patch = self.patch_tree.get_patch(*i);
            if offset >= self.patch_buffer_size {
                continue;
            }
            let [v0, v1, v2] = patch.points();
            let n0 = v0.coords.normalize();
            let n1 = v1.coords.normalize();
            let n2 = v2.coords.normalize();

            // project patch verts from global coordinates into view space.
            let v0 = scale * view.to_homogeneous() * v0.to_homogeneous();
            let v1 = scale * view.to_homogeneous() * v1.to_homogeneous();
            let v2 = scale * view.to_homogeneous() * v2.to_homogeneous();

            verts.push(PatchVertex::new(&Point3::from(v0.xyz()), &n0));
            verts.push(PatchVertex::new(&Point3::from(v1.xyz()), &n1));
            verts.push(PatchVertex::new(&Point3::from(v2.xyz()), &n2));

            dbg_indices.push(dbg_verts.len() as u32);
            dbg_indices.push(dbg_verts.len() as u32 + 1);
            dbg_indices.push(dbg_verts.len() as u32 + 2);
            let level = self.patch_tree.level_of_patch(*i);
            let clr = DBG_COLORS_BY_LEVEL[level];
            dbg_verts.push(DebugVertex::new(&Point3::from(v0.xyz()), &n0, &clr));
            dbg_verts.push(DebugVertex::new(&Point3::from(v1.xyz()), &n1, &clr));
            dbg_verts.push(DebugVertex::new(&Point3::from(v2.xyz()), &n2, &clr));
        }
        self.dbg_vertex_count = dbg_verts.len() as u32;
        //println!("verts: {}: {:?}", cnt, Instant::now() - loop_start);

        while verts.len() < 3 * self.patch_buffer_size {
            verts.push(PatchVertex::empty());
        }
        gpu.upload_slice_to(
            "terrain-geo-patch-vertex-upload-buffer",
            &verts,
            self.patch_vertex_buffer.clone(),
            wgpu::BufferUsage::all(),
            upload_buffers,
        );

        while dbg_verts.len() < DBG_VERT_COUNT {
            dbg_verts.push(DebugVertex {
                position: [0f32, 0f32, 0f32, 0f32],
                color: [0f32, 0f32, 1f32, 1f32],
            });
        }
        gpu.upload_slice_to(
            "terrain-geo-debug-vertices-upload-buffer",
            &dbg_verts,
            self.dbg_vertex_buffer.clone(),
            wgpu::BufferUsage::all(),
            upload_buffers,
        );
        gpu.upload_slice_to(
            "terrain-geo-debug-indices-upload-buffer",
            &dbg_indices,
            self.dbg_index_buffer.clone(),
            wgpu::BufferUsage::all(),
            upload_buffers,
        );

        //println!("dt: {:?}", Instant::now() - loop_start);
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
        self.patch_buffer_size as i32
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

    pub fn debug_index_buffer(&self) -> &wgpu::Buffer {
        &self.dbg_index_buffer
    }

    pub fn debug_vertex_buffer(&self) -> &wgpu::Buffer {
        &self.dbg_vertex_buffer
    }

    pub fn debug_index_range(&self) -> Range<u32> {
        0..self.dbg_vertex_count
        //0..(DBG_VERT_COUNT as u32 * 2u32)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_subdivision_vertex_counts() {
        let expect = vec![3, 6, 15, 45, 153, 561, 2145, 8385];
        for (i, &value) in expect.iter().enumerate() {
            assert_eq!(value, GpuDetailLevel::vertices_per_subdivision(i));
        }
    }
}
