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

use crate::patch_tree::PatchTree;
pub use crate::{debug_vertex::DebugVertex, patch_vertex::PatchVertex};

use camera::ArcBallCamera;
use failure::Fallible;
use frame_graph::CopyBufferDescriptor;
use gpu::GPU;
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
    // max-level, buffer-size, falloff-coefficient
    fn parameters(&self) -> (usize, f64, usize) {
        match self {
            Self::Low => (8, 0.8, 256),
            Self::Medium => (14, 0.8, 768),
        }
    }
}

pub struct TerrainGeoBuffer {
    // Maximum number of patches for the patch buffer.
    patch_buffer_size: usize,

    patches: PatchTree,

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
        _gen_subdivisions: usize,
        device: &wgpu::Device,
    ) -> Fallible<Arc<RefCell<Self>>> {
        let (max_level, falloff_coefficient, patch_buffer_size) = cpu_detail_level.parameters();
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

        let patches = PatchTree::new(max_level, falloff_coefficient);

        println!(
            "dbg_vertex_buffer: {:08X}",
            mem::size_of::<DebugVertex>() * DBG_VERT_COUNT
        );
        let dbg_vertex_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            size: (mem::size_of::<DebugVertex>() * DBG_VERT_COUNT) as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::all(),
        })));
        let mut dbg_indices = Vec::new();
        dbg_indices.push(0);
        for i in 1u32..DBG_VERT_COUNT as u32 {
            dbg_indices.push(i);
            dbg_indices.push(i);
            dbg_indices.push(i);
        }
        let dbg_index_buffer = Arc::new(Box::new(
            device
                .create_buffer_mapped(dbg_indices.len(), wgpu::BufferUsage::all())
                .fill_from_slice(&dbg_indices),
        ));

        println!(
            "patch_vertex_buffer: {:08X}",
            PatchVertex::mem_size() * 3 * patch_buffer_size
        );
        let patch_vertex_buffer =
            Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
                size: (PatchVertex::mem_size() * 3 * patch_buffer_size) as wgpu::BufferAddress,
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
            patch_buffer_size,
            patches,

            patch_vertex_buffer,
            patch_index_buffer,

            dbg_vertex_buffer,
            dbg_index_buffer,
            dbg_vertex_count: 0,
        })))
    }

    pub fn make_upload_buffer(
        &mut self,
        camera: &ArcBallCamera,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        let mut dbg_verts = Vec::with_capacity(3 * self.patch_buffer_size);
        let mut verts = Vec::with_capacity(3 * self.patch_buffer_size);
        let mut dbg_indices = Vec::with_capacity(3 * self.patch_buffer_size);
        let mut live_patches = Vec::with_capacity(self.patch_buffer_size);
        self.patches.optimize_for_view(camera, &mut live_patches);
        assert!(live_patches.len() < self.patch_buffer_size);

        for (offset, i) in live_patches.iter().enumerate() {
            let patch = self.patches.get_patch(*i);
            if offset >= self.patch_buffer_size {
                continue;
            }
            assert!(patch.is_alive());
            let [v0, v1, v2] = patch.points();
            let n0 = v0.coords.normalize();
            let n1 = v1.coords.normalize();
            let n2 = v2.coords.normalize();
            verts.push(PatchVertex::new(&v0, &n0));
            verts.push(PatchVertex::new(&v1, &n1));
            verts.push(PatchVertex::new(&v2, &n2));

            dbg_indices.push(dbg_verts.len() as u32);
            dbg_indices.push(dbg_verts.len() as u32 + 1);
            dbg_indices.push(dbg_verts.len() as u32 + 2);
            let level = self.patches.level_of_patch(*i);
            let clr = DBG_COLORS_BY_LEVEL[level];
            dbg_verts.push(DebugVertex::new(&v0, &n0, &clr));
            dbg_verts.push(DebugVertex::new(&v1, &n1, &clr));
            dbg_verts.push(DebugVertex::new(&v2, &n2, &clr));
        }
        self.dbg_vertex_count = dbg_verts.len() as u32;
        //println!("verts: {}: {:?}", cnt, Instant::now() - loop_start);

        while verts.len() < 3 * self.patch_buffer_size {
            verts.push(PatchVertex::empty());
        }
        let patch_vertex_buffer = gpu
            .device()
            .create_buffer_mapped(verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&verts);
        upload_buffers.push(CopyBufferDescriptor::new(
            patch_vertex_buffer,
            self.patch_vertex_buffer.clone(),
            (mem::size_of::<PatchVertex>() * verts.len()) as wgpu::BufferAddress,
        ));

        while dbg_verts.len() < DBG_VERT_COUNT {
            dbg_verts.push(DebugVertex {
                position: [0f32, 0f32, 0f32, 0f32],
                color: [0f32, 0f32, 1f32, 1f32],
            });
        }
        let debug_vertex_buffer = gpu
            .device()
            .create_buffer_mapped(dbg_verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&dbg_verts);
        upload_buffers.push(CopyBufferDescriptor::new(
            debug_vertex_buffer,
            self.dbg_vertex_buffer.clone(),
            (mem::size_of::<DebugVertex>() * dbg_verts.len()) as wgpu::BufferAddress,
        ));
        let debug_index_buffer = gpu
            .device()
            .create_buffer_mapped(dbg_indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&dbg_indices);
        upload_buffers.push(CopyBufferDescriptor::new(
            debug_index_buffer,
            self.dbg_index_buffer.clone(),
            (mem::size_of::<u32>() * dbg_indices.len()) as wgpu::BufferAddress,
        ));

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
