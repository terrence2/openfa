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
mod components;

pub use components::*;
pub use shape_chunk::{DrawSelection, DrawState};

use failure::Fallible;
use frame_graph::CopyBufferDescriptor;
use gpu::{DrawIndirectCommand, GPU};
use legion::prelude::*;
use lib::Library;
use log::trace;
use pal::Palette;
use shape_chunk::{ChunkId, ChunkPart, ShapeChunkBuffer, ShapeErrata, ShapeId, ShapeWidgets};
use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    mem,
    sync::Arc,
    time::Instant,
};
use universe::component::{Rotation, Scale, Transform};

pub const SHAPE_UNIT_TO_FEET: f32 = 4f32;

const BLOCK_SIZE: usize = 1 << 10;

type TransformType = [f32; 8];

thread_local! {
    pub static WIDGET_CACHE: RefCell<HashMap<ShapeId, ShapeWidgets>> = RefCell::new(HashMap::new());
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct BlockId(u32);

impl BlockId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SlotId {
    block_id: BlockId,
    offset: u32,
}

impl SlotId {
    fn new(block_id: BlockId, offset: u32) -> Self {
        Self { block_id, offset }
    }

    fn index(self) -> usize {
        self.offset as usize
    }
}

// Fixed reservation blocks for upload of a number of entities. Unfortunately, because of
// xforms, we don't know exactly how many instances will fit in any given block.
pub struct InstanceBlock {
    // Our own block id, for creating slots.
    block_id: BlockId,

    // Weak reference to the associated chunk in the Manager.
    chunk_id: ChunkId,

    // Current allocation head.
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
    transform_buffer_scratch: [TransformType; BLOCK_SIZE],
    flag_buffer_scratch: [[u32; 2]; BLOCK_SIZE],
    xform_index_buffer_scratch: [u32; BLOCK_SIZE],
    xform_buffer_scratch: [[f32; 6]; 14 * BLOCK_SIZE],

    command_buffer: Arc<Box<wgpu::Buffer>>,
    transform_buffer: Arc<Box<wgpu::Buffer>>,
    flag_buffer: Arc<Box<wgpu::Buffer>>,
    xform_index_buffer: Arc<Box<wgpu::Buffer>>,
    xform_buffer: Arc<Box<wgpu::Buffer>>,

    bind_group: wgpu::BindGroup,
}

impl InstanceBlock {
    fn new(
        block_id: BlockId,
        chunk_id: ChunkId,
        layout: &wgpu::BindGroupLayout,
        device: &wgpu::Device,
    ) -> Fallible<Self> {
        // This class contains the fixed-size device local blocks that we will render from.
        trace!("InstanceBlock::new({:?})", block_id);

        let command_buffer_size =
            (mem::size_of::<DrawIndirectCommand>() * BLOCK_SIZE) as wgpu::BufferAddress;
        let command_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-command-buffer"),
            size: command_buffer_size,
            usage: wgpu::BufferUsage::all(),
        })));

        let transform_buffer_size =
            (mem::size_of::<TransformType>() * BLOCK_SIZE) as wgpu::BufferAddress;
        let transform_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-xform-buffer"),
            size: transform_buffer_size,
            usage: wgpu::BufferUsage::all(),
        })));

        // TODO: Only allocate flag and xform buffers if the block requires them

        let flag_buffer_size = (mem::size_of::<[u32; 2]>() * BLOCK_SIZE) as wgpu::BufferAddress;
        let flag_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-flag-buffer"),
            size: flag_buffer_size,
            usage: wgpu::BufferUsage::all(),
        })));

        let xform_index_buffer_size = (mem::size_of::<u32>() * BLOCK_SIZE) as wgpu::BufferAddress;
        let xform_index_buffer =
            Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shape-instance-xform-index-buffer"),
                size: xform_index_buffer_size,
                usage: wgpu::BufferUsage::all(),
            })));

        let xform_buffer_size =
            (mem::size_of::<[f32; 6]>() * 14 * BLOCK_SIZE) as wgpu::BufferAddress;
        let xform_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape-instance-xform-buffer"),
            size: xform_buffer_size,
            usage: wgpu::BufferUsage::all(),
        })));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shape-instance-bind-group"),
            layout: &layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &transform_buffer,
                        range: 0..transform_buffer_size,
                    },
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &flag_buffer,
                        range: 0..flag_buffer_size,
                    },
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &xform_index_buffer,
                        range: 0..xform_index_buffer_size,
                    },
                },
                wgpu::Binding {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &xform_buffer,
                        range: 0..xform_buffer_size,
                    },
                },
            ],
        });

        Ok(Self {
            block_id,
            next_slot: 0,
            slot_map: Box::new([0; BLOCK_SIZE]),
            slots_dirty: false,
            xform_cursor: 0,
            chunk_id,
            command_buffer_scratch: [DrawIndirectCommand {
                vertex_count: 0,
                instance_count: 0,
                first_vertex: 0,
                first_instance: 0,
            }; BLOCK_SIZE],
            transform_buffer_scratch: [[0f32; 8]; BLOCK_SIZE],
            flag_buffer_scratch: [[0u32; 2]; BLOCK_SIZE],
            xform_index_buffer_scratch: [0u32; BLOCK_SIZE],
            xform_buffer_scratch: [[0f32; 6]; 14 * BLOCK_SIZE],
            command_buffer,
            transform_buffer,
            flag_buffer,
            xform_index_buffer,
            xform_buffer,
            bind_group,
        })
    }

    pub fn id(&self) -> BlockId {
        self.block_id
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn command_buffer(&self) -> &wgpu::Buffer {
        &self.command_buffer
    }

    fn has_open_slot(&self) -> bool {
        self.len() < BLOCK_SIZE
    }

    fn allocate_slot(&mut self, draw_cmd: DrawIndirectCommand) -> SlotId {
        assert!(self.has_open_slot());
        let slot_id = SlotId::new(self.block_id, self.next_slot);
        self.next_slot += 1;

        // Slots always start pointing at themselves -- as we remove entities and GC,
        // stuff will get mixed around. Also mark ourself as dirty so that we will get
        // uploaded, once we enter the draw portion.
        self.slot_map[slot_id.index()] = slot_id.index();
        self.command_buffer_scratch[slot_id.index()] = draw_cmd;
        self.slots_dirty = true;

        slot_id
    }

    #[inline]
    fn begin_frame(&mut self) {
        self.xform_cursor = 0;
    }

    #[inline]
    fn push_values(
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

    pub fn chunk_id(&self) -> ChunkId {
        self.chunk_id
    }

    pub fn len(&self) -> usize {
        self.next_slot as usize
    }

    pub fn is_empty(&self) -> bool {
        self.next_slot == 0
    }

    /*
    // We expect entities to be added and removed frequently, but not every entity on every frame.
    // We take some pains here to make sure we only copy change entities up to the GPU each frame.
    fn allocate_entity_slot(
        &mut self,
        chunk_part: &ChunkPart,
        mut draw_command: DrawIndirectCommand,
    ) -> Option<SlotId> {
        if chunk_index != self.chunk_index {
            return None; // block not for this chunk
        }

        // GC compacts all ids so that allocation can just bump.
        if self.reservation_offset >= BLOCK_SIZE {
            return None; // block full
        }
        let slot = SlotId(self.reservation_offset);
        self.reservation_offset += 1;

        // Update the draw commands and add an upload slot for the new one.
        draw_command.first_instance = slot.0 as u32;
        self.command_buffer_scratch[slot.0] = draw_command;
        self.command_upload_set.push(slot.0);

        Some(slot)
    }

    // Lookup the existing slot (may be newly created), return the slot index for direct O(1)
    // access to all of the scratch buffers we need to update. Also marks the entity as alive.
    fn get_existing_slot(&mut self, id: EntityId) -> SlotId {
        let slot = self.entity_to_slot_map[&id];
        self.mark_buffer[slot.0] = true;
        slot
    }

    // Maintain is our GC routine. It must be called at the end of every frame.
    fn maintain(&mut self, removed: &mut Vec<EntityId>) -> bool {
        fn swap<T>(arr: &mut [T], a: usize, b: usize)
        where
            T: Copy,
        {
            let tmp = arr[a];
            arr[a] = arr[b];
            arr[b] = tmp;
        }

        let mut head = 0;
        let mut tail = self.reservation_offset - 1;

        // Already empty; should not be reachable because we should already be cleaned.
        assert!(tail >= head);

        // Since we're using swapping, we need to handle the last element specially.
        if head == tail && !self.mark_buffer[head] {
            return true;
        }

        // Walk from the front of the buffer to the end of the buffer. If an entity is not marked,
        // swap it with the tail and shrink the buffer.
        while head < tail {
            if !self.mark_buffer[head] {
                swap(&mut self.mark_buffer, head, tail);
                swap(&mut self.command_buffer_scratch, head, tail);
                swap(&mut self.transform_buffer_scratch, head, tail);
                swap(&mut self.flag_buffer_scratch, head, tail);
                swap(&mut self.xform_index_buffer_scratch, head, tail);
                tail -= 1;
            } else {
                // Note that this is an `else` because we need to re-check head in case tail
                // was itself unmarked.
                self.mark_buffer[head] = false;
                head += 1;
            }
        }

        self.reservation_offset = tail + 1;
        return false;
    }

    fn get_transform_buffer_slot(&mut self, slot_index: SlotId) -> &mut [f32; 6] {
        &mut self.transform_buffer_scratch[slot_index.0]
    }

    fn get_flag_buffer_slot(&mut self, slot_index: SlotId) -> &mut [u32; 2] {
        &mut self.flag_buffer_scratch[slot_index.0]
    }

    fn get_xform_buffer_slot(&mut self, slot_index: SlotId) -> &mut [f32] {
        &mut self.xform_buffer_scratch[self.xform_index_buffer_scratch[slot_index.0] as usize]
    }

    fn update_buffers(
        &self,
        mut cbb: AutoCommandBufferBuilder,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let dic = self.command_buffer_scratch.to_vec();
        let command_buffer_upload = self.command_buffer_pool.chunk(dic)?;
        cbb = cbb.copy_buffer(command_buffer_upload, self.command_buffer.clone())?;

        let tr = self.transform_buffer_scratch.to_vec();
        let transform_buffer_upload = self.transform_buffer_pool.chunk(tr)?;
        cbb = cbb.copy_buffer(transform_buffer_upload, self.transform_buffer.clone())?;

        let fl = self.flag_buffer_scratch.to_vec();
        let flag_buffer_upload = self.flag_buffer_pool.chunk(fl)?;
        cbb = cbb.copy_buffer(flag_buffer_upload, self.flag_buffer.clone())?;

        let xfi = self.xform_index_buffer_scratch.to_vec();
        let xform_index_buffer_upload = self.xform_index_buffer_pool.chunk(xfi)?;
        cbb = cbb.copy_buffer(xform_index_buffer_upload, self.xform_index_buffer.clone())?;

        Ok(cbb)
    }

    pub fn render(
        &self,
        cbb: AutoCommandBufferBuilder,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        chunk: &ClosedChunk,
        camera: &dyn CameraAbstract,
        window: &GraphicsWindow,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let mut push_constants = vs::ty::PushConstantData::new();
        push_constants.set_projection(&camera.projection_matrix());
        push_constants.set_view(&camera.view_matrix());

        let ib = self.command_buffer.clone();
        Ok(cbb.draw_indirect(
            pipeline.clone(),
            &window.dynamic_state,
            vec![chunk.vertex_buffer()],
            ib.into_buffer_slice()
                .slice(0..self.reservation_offset)
                .unwrap(),
            (
                self.pds0.clone(),
                self.pds1.clone(),
                self.pds2.clone(),
                self.descriptor_set.clone(),
                chunk.atlas_descriptor_set_ref(),
            ),
            push_constants,
        )?)
    }
    */
}

pub struct ShapeInstanceBuffer {
    pub chunk_man: ShapeChunkBuffer,

    chunk_to_block_map: HashMap<ChunkId, Vec<BlockId>>,
    pub blocks: HashMap<BlockId, InstanceBlock>,
    next_block_id: u32,

    bind_group_layout: wgpu::BindGroupLayout,
}

impl ShapeInstanceBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Arc<RefCell<Self>>> {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shape-instance-bind-group-layout"),
            bindings: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                },
            ],
        });

        Ok(Arc::new(RefCell::new(Self {
            chunk_man: ShapeChunkBuffer::new(device)?,
            chunk_to_block_map: HashMap::new(),
            blocks: HashMap::new(),
            next_block_id: 0,
            bind_group_layout,
        })))
    }

    pub fn block(&self, id: &BlockId) -> &InstanceBlock {
        &self.blocks[id]
    }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        self.chunk_man.part(shape_id)
    }

    pub fn errata(&self, shape_id: ShapeId) -> ShapeErrata {
        self.chunk_man
            .part(shape_id)
            .widgets()
            .read()
            .unwrap()
            .errata()
    }

    fn allocate_block_id(&mut self) -> BlockId {
        assert!(self.next_block_id < std::u32::MAX);
        let bid = self.next_block_id;
        self.next_block_id += 1;
        BlockId(bid)
    }

    fn find_open_block(&mut self, chunk_id: ChunkId) -> Option<BlockId> {
        if let Some(blocks) = self.chunk_to_block_map.get(&chunk_id) {
            for block_id in blocks {
                if self.blocks[block_id].has_open_slot() {
                    return Some(*block_id);
                }
            }
        }
        None
    }

    pub fn upload_and_allocate_slot(
        &mut self,
        name: &str,
        selection: DrawSelection,
        palette: &Palette,
        lib: &Library,
        gpu: &mut GPU,
    ) -> Fallible<(ShapeId, SlotId)> {
        // Ensure that the shape is actually in a chunk somewhere.
        let (chunk_id, shape_id) = self
            .chunk_man
            .upload_shape(name, selection, &palette, &lib, gpu)?;

        // Find or create a block that we can use to track the instance data.
        let block_id = if let Some(block_id) = self.find_open_block(chunk_id) {
            block_id
        } else {
            let block_id = self.allocate_block_id();
            let block =
                InstanceBlock::new(block_id, chunk_id, &self.bind_group_layout, gpu.device())?;
            self.chunk_to_block_map
                .entry(chunk_id)
                .or_insert_with(Vec::new)
                .push(block_id);
            self.blocks.insert(block_id, block);
            block_id
        };

        // FIXME: this is less useful than I thought it would be.
        let draw_cmd = self
            .chunk_man
            .part(shape_id)
            .draw_command(self.blocks[&block_id].len() as u32, 1);
        let slot_id = self
            .blocks
            .get_mut(&block_id)
            .unwrap()
            .allocate_slot(draw_cmd);

        Ok((shape_id, slot_id))
    }

    pub fn ensure_uploaded(&mut self, gpu: &mut GPU) -> Fallible<()> {
        self.chunk_man.finish_open_chunks(gpu)
    }

    #[inline]
    pub fn push_values(
        &mut self,
        slot_id: SlotId,
        transform: &TransformType,
        flags: [u32; 2],
        xforms: &Option<[[f32; 6]; 14]>,
        xform_count: usize,
    ) {
        self.blocks.get_mut(&slot_id.block_id).unwrap().push_values(
            slot_id,
            transform,
            flags,
            xforms,
            xform_count,
        );
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn make_upload_buffer(
        &mut self,
        start: &Instant,
        world: &mut World,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        let now = Instant::now();

        // Reset cursor for our next upload.
        for block in self.blocks.values_mut() {
            block.begin_frame();
        }

        // Animate the draw_state. We'll use the updated values below when computing
        // xform and frame based animation states.
        <Write<ShapeState>>::query().par_for_each(world, |mut shape_state| {
            shape_state.draw_state.animate(&now)
        });

        let query = <(
            Read<Transform>,
            Read<Rotation>,
            Read<Scale>,
            Write<ShapeTransformBuffer>,
        )>::query();
        // TODO: distinguish first run, as it doesn't seem to see "new" as changed.
        //    .filter(changed::<Transform>() | changed::<Rotation>());
        query.par_for_each(
            world,
            |(transform, rotation, scale, mut transform_buffer)| {
                (&mut transform_buffer.buffer[0..3]).copy_from_slice(&transform.compact());
                (&mut transform_buffer.buffer[3..6]).copy_from_slice(&rotation.compact());
                (&mut transform_buffer.buffer[6..7]).copy_from_slice(&scale.compact());
            },
        );

        let query = <(Read<ShapeState>, Write<ShapeFlagBuffer>)>::query();
        query.par_for_each(world, |(shape_state, mut flag_buffer)| {
            shape_state
                .draw_state
                .build_mask_into(&start, &mut flag_buffer.buffer)
                .unwrap();
        });

        let query = <(Read<ShapeRef>, Read<ShapeState>, Write<ShapeXformBuffer>)>::query();
        query.par_for_each(world, |(shape_ref, shape_state, mut xform_buffer)| {
            let part = self.chunk_man.part(shape_ref.shape_id);
            WIDGET_CACHE.with(|widget_cache| {
                match widget_cache.borrow_mut().entry(shape_ref.shape_id) {
                    Entry::Occupied(mut e) => {
                        e.get_mut()
                            .animate_into(
                                &shape_state.draw_state,
                                &start,
                                &now,
                                &mut xform_buffer.buffer,
                            )
                            .unwrap();
                    }
                    Entry::Vacant(e) => {
                        let mut widgets = part.widgets().read().unwrap().clone();
                        widgets
                            .animate_into(
                                &shape_state.draw_state,
                                &start,
                                &now,
                                &mut xform_buffer.buffer,
                            )
                            .unwrap();
                        e.insert(widgets);
                    }
                }
            });
        });

        let query = <(
            Read<ShapeRef>,
            Read<ShapeSlot>,
            Read<ShapeTransformBuffer>,
            Read<ShapeFlagBuffer>,
            TryRead<ShapeXformBuffer>,
        )>::query();
        for (shape_ref, shape_slot, transform_buffer, flag_buffer, xform_buffer) in
            query.iter(world)
        {
            let xform_count = self.chunk_man.part(shape_ref.shape_id).xform_count();
            self.push_values(
                shape_slot.slot_id,
                &transform_buffer.buffer,
                flag_buffer.buffer,
                &xform_buffer.map(|b| b.buffer),
                xform_count,
            );
        }

        for block in self.blocks.values() {
            let source = gpu.push_slice(
                "shape-instance-command-buffer-scratch",
                &block.command_buffer_scratch[..block.len()],
                wgpu::BufferUsage::all(),
            );
            upload_buffers.push(CopyBufferDescriptor::new(
                source,
                block.command_buffer.clone(),
                (mem::size_of::<DrawIndirectCommand>() * block.len()) as wgpu::BufferAddress,
            ));

            let source = gpu.push_slice(
                "shape-instance-transform-buffer-scratch",
                &block.transform_buffer_scratch[..block.len()],
                wgpu::BufferUsage::all(),
            );
            upload_buffers.push(CopyBufferDescriptor::new(
                source,
                block.transform_buffer.clone(),
                (mem::size_of::<TransformType>() * block.len()) as wgpu::BufferAddress,
            ));

            let source = gpu.push_slice(
                "shape-instance-flag-buffer-scratch",
                &block.flag_buffer_scratch[..block.len()],
                wgpu::BufferUsage::all(),
            );
            upload_buffers.push(CopyBufferDescriptor::new(
                source,
                block.flag_buffer.clone(),
                (mem::size_of::<[u32; 2]>() * block.len()) as wgpu::BufferAddress,
            ));

            let source = gpu.push_slice(
                "shape-instance-xform-index-buffer-scratch",
                &block.xform_index_buffer_scratch[..block.len()],
                wgpu::BufferUsage::all(),
            );
            upload_buffers.push(CopyBufferDescriptor::new(
                source,
                block.xform_index_buffer.clone(),
                (mem::size_of::<u32>() * block.len()) as wgpu::BufferAddress,
            ));

            let source = gpu.push_slice(
                "shape-instance-xform-buffer-scratch",
                &block.xform_buffer_scratch[..block.xform_cursor],
                wgpu::BufferUsage::all(),
            );
            upload_buffers.push(CopyBufferDescriptor::new(
                source,
                block.xform_buffer.clone(),
                (mem::size_of::<[f32; 6]>() * block.xform_cursor) as wgpu::BufferAddress,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use input::InputSystem;
    use omnilib::OmniLib;
    use pal::Palette;
    use shape_chunk::DrawSelection;

    #[test]
    fn test_creation() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let lib = omni.library("FA");
        let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let inst_man = ShapeInstanceBuffer::new(gpu.device())?;

        for _ in 0..100 {
            let (_chunk_id, _slot_id) = inst_man.borrow_mut().upload_and_allocate_slot(
                "T80.SH",
                DrawSelection::NormalModel,
                &palette,
                &lib,
                &mut gpu,
            )?;
        }

        Ok(())
    }
}
