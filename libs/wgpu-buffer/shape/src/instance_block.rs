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
use crate::chunk::DrawIndirectCommand;
use bevy_ecs::prelude::*;
use gpu::Gpu;
use log::trace;
use std::{mem, num::NonZeroU64, sync::Arc};

const BLOCK_SIZE: usize = 1 << 10;

pub(crate) type TransformType = [f32; 8];

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct BlockId(u32);

impl BlockId {
    pub(crate) fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Component, Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SlotId {
    block_id: BlockId,
    offset: u32,
}

impl SlotId {
    fn new(block_id: BlockId, offset: u32) -> Self {
        Self { block_id, offset }
    }

    pub(crate) fn block_id(&self) -> &BlockId {
        &self.block_id
    }

    fn index(self) -> usize {
        self.offset as usize
    }
}

// Fixed reservation blocks for upload of a number of entities. Unfortunately, because of
// xforms, we don't know exactly how many instances will fit in any given block.
#[derive(Debug)]
pub struct InstanceBlock {
    // Our own block id, for creating slots.
    block_id: BlockId,

    // Current allocation head.
    free_slot: usize,
    next_slot: u32,

    // Map from slot offset to the actual storage location. This must be derefed to
    // know the actual offset into the relevant buffer.
    slot_map: Box<[usize; BLOCK_SIZE]>,

    // Flag set whenever the slot map changes and we need to re-up the command_buffer.
    slots_dirty: bool,

    // Cursor for insertion into the xforms buffer.
    xform_cursor: usize,

    // Map from the entity to the stored offset and from the offset to the entity.
    //    reservation_offset: usize,       // bump head
    //    mark_buffer: [bool; BLOCK_SIZE], // GC marked set
    //    descriptor_set: Arc<dyn DescriptorSet + Send + Sync>,
    //
    pub command_buffer_scratch: [DrawIndirectCommand; BLOCK_SIZE],
    transform_buffer_scratch: Box<[TransformType; BLOCK_SIZE]>,
    flag_buffer_scratch: Box<[[u32; 2]; BLOCK_SIZE]>,
    xform_index_buffer_scratch: Box<[u32; BLOCK_SIZE]>,
    xform_buffer_scratch: Box<[[f32; 6]; 14 * BLOCK_SIZE]>,

    command_buffer: Arc<wgpu::Buffer>,
    transform_buffer: Arc<wgpu::Buffer>,
    flag_buffer: Arc<wgpu::Buffer>,
    xform_index_buffer: Arc<wgpu::Buffer>,
    xform_buffer: Arc<wgpu::Buffer>,

    bind_group: wgpu::BindGroup,
}

impl InstanceBlock {
    const TRANSFORM_BUFFER_SIZE: wgpu::BufferAddress =
        (mem::size_of::<TransformType>() * BLOCK_SIZE) as wgpu::BufferAddress;
    const FLAG_BUFFER_SIZE: wgpu::BufferAddress =
        (mem::size_of::<[u32; 2]>() * BLOCK_SIZE) as wgpu::BufferAddress;
    const XFORM_INDEX_BUFFER_SIZE: wgpu::BufferAddress =
        (mem::size_of::<u32>() * BLOCK_SIZE) as wgpu::BufferAddress;
    const XFORM_BUFFER_SIZE: wgpu::BufferAddress =
        (mem::size_of::<[f32; 6]>() * 14 * BLOCK_SIZE) as wgpu::BufferAddress;

    pub(crate) const fn transform_buffer_size() -> Option<NonZeroU64> {
        NonZeroU64::new(Self::TRANSFORM_BUFFER_SIZE)
    }

    pub(crate) const fn flag_buffer_size() -> Option<NonZeroU64> {
        NonZeroU64::new(Self::FLAG_BUFFER_SIZE)
    }

    pub(crate) const fn xform_index_buffer_size() -> Option<NonZeroU64> {
        NonZeroU64::new(Self::XFORM_INDEX_BUFFER_SIZE)
    }

    pub(crate) const fn xform_buffer_size() -> Option<NonZeroU64> {
        NonZeroU64::new(Self::XFORM_BUFFER_SIZE)
    }

    pub(crate) fn new(
        block_id: BlockId,
        layout: &wgpu::BindGroupLayout,
        device: &wgpu::Device,
    ) -> Self {
        // This class contains the fixed-size device local blocks that we will render from.
        trace!("InstanceBlock::new({:?})", block_id);

        let command_buffer_size =
            (mem::size_of::<DrawIndirectCommand>() * BLOCK_SIZE) as wgpu::BufferAddress;
        let command_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-command-buffer"),
            size: command_buffer_size,
            usage: wgpu::BufferUsages::all(),
            mapped_at_creation: false,
        }));

        let transform_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-xform-buffer"),
            size: Self::TRANSFORM_BUFFER_SIZE,
            usage: wgpu::BufferUsages::all(),
            mapped_at_creation: false,
        }));

        // TODO: Only allocate flag and xform buffers if the block requires them

        let flag_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-flag-buffer"),
            size: Self::FLAG_BUFFER_SIZE,
            usage: wgpu::BufferUsages::all(),
            mapped_at_creation: false,
        }));

        let xform_index_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-xform-index-buffer"),
            size: Self::XFORM_INDEX_BUFFER_SIZE,
            usage: wgpu::BufferUsages::all(),
            mapped_at_creation: false,
        }));

        let xform_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-xform-buffer"),
            size: Self::XFORM_BUFFER_SIZE,
            usage: wgpu::BufferUsages::all(),
            mapped_at_creation: false,
        }));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shape-instance-bind-group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &transform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &flag_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &xform_index_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &xform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        Self {
            block_id,
            free_slot: 0,
            next_slot: 0,
            slot_map: Box::new([0; BLOCK_SIZE]),
            slots_dirty: false,
            xform_cursor: 0,
            command_buffer_scratch: [DrawIndirectCommand {
                vertex_count: 0,
                instance_count: 0,
                first_vertex: 0,
                first_instance: 0,
            }; BLOCK_SIZE],
            transform_buffer_scratch: Box::new([[0f32; 8]; BLOCK_SIZE]),
            flag_buffer_scratch: Box::new([[0u32; 2]; BLOCK_SIZE]),
            xform_index_buffer_scratch: Box::new([0u32; BLOCK_SIZE]),
            xform_buffer_scratch: Box::new([[0f32; 6]; 14 * BLOCK_SIZE]),
            command_buffer,
            transform_buffer,
            flag_buffer,
            xform_index_buffer,
            xform_buffer,
            bind_group,
        }
    }

    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub(crate) fn has_open_slot(&self) -> bool {
        self.free_slot < BLOCK_SIZE
    }

    pub(crate) fn allocate_slot(&mut self, draw_cmd: DrawIndirectCommand) -> SlotId {
        assert!(self.has_open_slot());
        let slot_id = SlotId::new(self.block_id, self.next_slot);
        self.next_slot += 1;

        // Slots always start pointing at themselves -- as we remove entities and GC,
        // stuff will get mixed around. Also mark ourself as dirty so that we will get
        // uploaded, once we enter the draw portion.
        self.command_buffer_scratch[self.free_slot] = draw_cmd;
        self.slot_map[slot_id.index()] = self.free_slot;
        self.free_slot += 1;
        self.slots_dirty = true;

        slot_id
    }

    pub(crate) fn deallocate_slot(&mut self, slot_id: SlotId) {
        assert_eq!(self.block_id, slot_id.block_id, "wrong block in free");

        // Swap tail with slot_id and zero tail
        assert!(self.free_slot > 0, "freeing with no free blocks");
        self.free_slot -= 1;
        if slot_id.index() != self.free_slot {
            self.command_buffer_scratch[slot_id.index()] =
                self.command_buffer_scratch[self.free_slot];
            self.slot_map[self.free_slot] = slot_id.index();
        }
        self.command_buffer_scratch[self.free_slot] = DrawIndirectCommand::default();
        self.slots_dirty = true;
    }

    #[inline]
    pub(crate) fn begin_frame(&mut self) {
        self.xform_cursor = 0;
    }

    #[inline]
    pub(crate) fn push_values(
        &mut self,
        slot_id: SlotId,
        transform: &TransformType,
        flags: [u32; 2],
        xforms: &Option<[[f32; 6]; 14]>,
        xform_count: usize,
    ) {
        let offset = self.slot_map[slot_id.index()];
        self.transform_buffer_scratch[offset] = *transform;
        self.flag_buffer_scratch[offset] = flags;
        if let Some(xf) = xforms {
            self.xform_index_buffer_scratch[offset] = self.xform_cursor as u32;
            self.xform_buffer_scratch[self.xform_cursor..self.xform_cursor + xform_count]
                .copy_from_slice(&xf[0..xform_count]);
            self.xform_cursor += xform_count;
        }
    }

    pub(crate) fn make_upload_buffer(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        gpu.upload_slice_to(
            "shape-instance-command-buffer-scratch",
            &self.command_buffer_scratch[..self.len()],
            self.command_buffer.clone(),
            encoder,
        );

        gpu.upload_slice_to(
            "shape-instance-transform-buffer-scratch",
            &self.transform_buffer_scratch[..self.len()],
            self.transform_buffer.clone(),
            encoder,
        );

        gpu.upload_slice_to(
            "shape-instance-flag-buffer-scratch",
            &self.flag_buffer_scratch[..self.len()],
            self.flag_buffer.clone(),
            encoder,
        );

        gpu.upload_slice_to(
            "shape-instance-xform-index-buffer-scratch",
            &self.xform_index_buffer_scratch[..self.len()],
            self.xform_index_buffer.clone(),
            encoder,
        );

        gpu.upload_slice_to(
            "shape-instance-xform-buffer-scratch",
            &self.xform_buffer_scratch[..self.xform_cursor],
            self.xform_buffer.clone(),
            encoder,
        );
    }

    pub(crate) fn len(&self) -> usize {
        self.free_slot
    }
}
