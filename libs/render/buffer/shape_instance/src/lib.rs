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
use bit_set::BitSet;
use failure::Fallible;
use global_layout::GlobalSets;
use lib::Library;
use pal::Palette;
use shape_chunk::{
    ChunkId, ChunkPart, ClosedChunk, DrawSelection, ShapeChunkManager, ShapeId, Vertex,
};
use specs::prelude::*;
use std::{collections::HashMap, sync::Arc, time::Instant};
use vulkano::{
    buffer::{BufferAccess, BufferUsage, CpuBufferPool, DeviceLocalBuffer},
    command_buffer::{AutoCommandBufferBuilder, DrawIndirectCommand, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    framebuffer::Subpass,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
    sync::GpuFuture,
};
use window::GraphicsWindow;

const BLOCK_SIZE: usize = 1 << 14;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SlotId(usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct BlockId(u32);

// Fixed reservation blocks for upload of a number of entities. Unfortunately, because of
// xforms, we don't know exactly how many instances will fit in any given block.
pub struct InstanceBlock {
    // Weak reference to the associated chunk in the Manager.
    chunk_id: ChunkId,

    // Current allocation head.
    next_slot: usize,
    slot_map: Box<[Option<SlotId>; BLOCK_SIZE]>,
    dirty_slots: BitSet,

    // Map from the entity to the stored offset and from the offset to the entity.
    //    reservation_offset: usize,       // bump head
    //    mark_buffer: [bool; BLOCK_SIZE], // GC marked set
    descriptor_set: Arc<dyn DescriptorSet + Send + Sync>,

    command_buffer: Arc<DeviceLocalBuffer<[DrawIndirectCommand]>>,
    transform_buffer: Arc<DeviceLocalBuffer<[[f32; 6]]>>,
    flag_buffer: Arc<DeviceLocalBuffer<[[u32; 2]]>>,
    xform_index_buffer: Arc<DeviceLocalBuffer<[u32]>>,
    /*
    // Buffers for all instances stored in this instance set. One command per unique entity.
    // 16 bytes per entity; index unnecessary for draw
    command_buffer_scratch: [DrawIndirectCommand; BLOCK_SIZE],
    command_upload_set: Vec<usize>,
    command_buffer_pool: CpuBufferPool<DrawIndirectCommand>,

    // Base position and orientation in xyz+euler angles stored as 6 adjacent floats.
    // 24 bytes per entity; buffer index inferable from drawing index
    transform_buffer_scratch: [[f32; 6]; BLOCK_SIZE],
    transform_buffer_pool: CpuBufferPool<[f32; 6]>,

    // 2 32bit flags words for each entity.
    // 8 bytes per entity; buffer index inferable from drawing index
    flag_buffer_scratch: [[u32; 2]; BLOCK_SIZE],
    flag_buffer_pool: CpuBufferPool<[u32; 2]>,

    // 4 bytes per entity; can infer position from index
    xform_index_buffer_scratch: [u32; BLOCK_SIZE],
    xform_index_buffer_pool: CpuBufferPool<u32>,

    // 0 to 14 position/orientation [f32; 6], depending on the shape.
    // assume 96 bytes per entity if we're talking about planes
    // cannot infer position, so needs an index buffer
    xform_buffer_scratch: [[f32; 6]; 14 * BLOCK_SIZE],
    xform_buffer_pool: CpuBufferPool<[f32; 6]>,
    xform_buffer: Arc<DeviceLocalBuffer<[[f32; 6]]>>,
    */
}

impl InstanceBlock {
    fn new(
        chunk_id: ChunkId,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        device: Arc<Device>,
    ) -> Fallible<Self> {
        // This class contains the fixed-size device local blocks that we will render from.

        // Static: except when things change
        let command_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;

        // Only need to upload for items that change position.
        // TODO: split chunks between buildings and movers and reflect that distinction
        // TODO: down into the block (here) so that we can ignore transform in most blocks.
        let transform_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;

        // Needs to be updated every frame for planes and for things that have animations.
        // TODO: split out animation flags and make others only required for planes
        let flag_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;

        // We re-run and rebuild the xforms buffer every frame, so we may need to re-build
        // the xform index buffer every frame too. It's not clear how long positions will hold.
        let xform_index_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;

        let descriptor_set = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::ShapeBuffers.into())
                .add_buffer(transform_buffer.clone())?
                .add_buffer(flag_buffer.clone())?
                .add_buffer(xform_index_buffer.clone())?
                .build()?,
        );

        Ok(Self {
            next_slot: 0,
            slot_map: Box::new([None; BLOCK_SIZE]),
            dirty_slots: BitSet::new(),
            chunk_id,
            descriptor_set,
            command_buffer,
            transform_buffer,
            flag_buffer,
            xform_index_buffer,
        })
    }

    fn has_open_slot(&self) -> bool {
        self.next_slot < BLOCK_SIZE
    }

    fn allocate_slot(&mut self, draw_command: DrawIndirectCommand) -> SlotId {
        assert!(self.has_open_slot());
        let slot_id = SlotId(self.next_slot);
        self.next_slot += 1;

        // Slots always start pointing at themselves -- as we remove entities and GC,
        // stuff will get mixed around. Also mark ourself as dirty so that we will get
        // uploaded, once we enter the draw portion.
        self.slot_map[slot_id.0] = Some(slot_id);
        self.dirty_slots.insert(slot_id.0);

        slot_id
    }

    /*
    fn new(
        chunk_index: ChunkId,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        base_descriptors: &[Arc<dyn DescriptorSet + Send + Sync>],
        command_buffer_pool: CpuBufferPool<DrawIndirectCommand>,
        transform_buffer_pool: CpuBufferPool<[f32; 6]>,
        flag_buffer_pool: CpuBufferPool<[u32; 2]>,
        xform_index_buffer_pool: CpuBufferPool<u32>,
        xform_buffer_pool: CpuBufferPool<[f32; 6]>,
        device: Arc<Device>,
    ) -> Fallible<Self> {
        let command_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;
        let transform_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;
        let flag_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;
        let xform_index_buffer = DeviceLocalBuffer::array(
            device.clone(),
            BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;
        let xform_buffer = DeviceLocalBuffer::array(
            device.clone(),
            14 * BLOCK_SIZE,
            BufferUsage::all(),
            device.active_queue_families(),
        )?;
        let descriptor_set = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::ShapeBuffers.into())
                .add_buffer(transform_buffer.clone())?
                .add_buffer(flag_buffer.clone())?
                .add_buffer(xform_buffer.clone())?
                .add_buffer(xform_index_buffer.clone())?
                .build()?,
        );
        Ok(Self {
            chunk_index,
            reservation_offset: 0,
            mark_buffer: [false; BLOCK_SIZE],
            descriptor_set,
            pds0: base_descriptors[0].clone(),
            pds1: base_descriptors[1].clone(),
            pds2: base_descriptors[2].clone(),
            command_buffer_scratch: [DrawIndirectCommand {
                vertex_count: 0u32,
                instance_count: 0u32,
                first_vertex: 0u32,
                first_instance: 0u32,
            }; BLOCK_SIZE],
            command_buffer_pool,
            command_upload_set: Vec::new(),
            command_buffer,
            transform_buffer_scratch: [[0f32; 6]; BLOCK_SIZE],
            transform_buffer_pool,
            transform_buffer,
            flag_buffer_scratch: [[0u32; 2]; BLOCK_SIZE],
            flag_buffer_pool,
            flag_buffer,
            xform_index_buffer_scratch: [0u32; BLOCK_SIZE],
            xform_index_buffer_pool,
            xform_index_buffer,
            xform_buffer_scratch: [[0f32; 6]; 14 * BLOCK_SIZE],
            xform_buffer_pool,
            xform_buffer,
        })
    }

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

pub struct ShapeInstanceManager {
    chunk_man: ShapeChunkManager,

    chunk_to_block_map: HashMap<ChunkId, Vec<BlockId>>,
    blocks: HashMap<BlockId, InstanceBlock>,
    next_block_id: u32,

    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    base_descriptors: [Arc<dyn DescriptorSet + Send + Sync>; 3],

    // Buffer pools are shared by all blocks for maximum re-use.
    command_buffer_pool: CpuBufferPool<DrawIndirectCommand>,
    transform_buffer_pool: CpuBufferPool<[f32; 6]>,
    flag_buffer_pool: CpuBufferPool<[u32; 2]>,
    xform_index_buffer_pool: CpuBufferPool<u32>,
    xform_buffer_pool: CpuBufferPool<[f32; 6]>, // FIXME: hunt down this max somewhere
}

impl ShapeInstanceManager {
    pub fn new(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        base_descriptors: [Arc<dyn DescriptorSet + Send + Sync>; 3],
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        Ok(Self {
            chunk_man: ShapeChunkManager::new(pipeline.clone(), &window)?,
            chunk_to_block_map: HashMap::new(),
            blocks: HashMap::new(),
            next_block_id: 0,
            pipeline,
            base_descriptors,
            command_buffer_pool: CpuBufferPool::new(window.device(), BufferUsage::all()),
            transform_buffer_pool: CpuBufferPool::new(window.device(), BufferUsage::all()),
            flag_buffer_pool: CpuBufferPool::new(window.device(), BufferUsage::all()),
            xform_index_buffer_pool: CpuBufferPool::new(window.device(), BufferUsage::all()),
            xform_buffer_pool: CpuBufferPool::new(window.device(), BufferUsage::all()),
        })
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
        window: &GraphicsWindow,
    ) -> Fallible<SlotId> {
        // Ensure that the shape is actually in a chunk somewhere.
        let (chunk_id, shape_id, fut) = self.chunk_man.upload_shape(
            "T80.SH",
            DrawSelection::NormalModel,
            &palette,
            &lib,
            &window,
        )?;

        // Find or create a block that we can use to track the instance data.
        let block_id = if let Some(block_id) = self.find_open_block(chunk_id) {
            block_id
        } else {
            let block_id = self.allocate_block_id();
            let block = InstanceBlock::new(chunk_id, self.pipeline.clone(), window.device())?;
            self.blocks.insert(block_id, block);
            block_id
        };
        let part = self.chunk_man.part(shape_id);
        let slot_id = self
            .blocks
            .get_mut(&block_id)
            .unwrap()
            .allocate_slot(part.draw_command(0, 1));

        Ok(slot_id)
    }

    pub fn ensure_finished(
        &mut self,
        window: &GraphicsWindow,
    ) -> Fallible<Option<Box<dyn GpuFuture>>> {
        self.chunk_man.finish(window)
    }

    pub fn render<T>(
        &self,
        mut cbb: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
        push_consts: &T,
    ) -> Fallible<AutoCommandBufferBuilder> {
        for block in self.blocks.values() {
            let chunk = &self.chunk_man.chunk(block.chunk_id);
            cbb = cbb.draw_indirect(
                self.pipeline.clone(),
                dynamic_state,
                vec![chunk.vertex_buffer().clone()],
                block.command_buffer.clone(),
                (
                    self.base_descriptors[0].clone(),
                    self.base_descriptors[1].clone(),
                    self.base_descriptors[2].clone(),
                    chunk.atlas_descriptor_set_ref(),
                    block.descriptor_set.clone(),
                ),
                push_consts,
            )?;
        }
        Ok(cbb)
    }
}

pub struct ShapeComponent {
    slot_id: SlotId,
}
impl Component for ShapeComponent {
    type Storage = VecStorage<Self>;
}
impl ShapeComponent {
    pub fn new(slot_id: SlotId) -> Self {
        Self { slot_id }
    }
}

mod test_vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
    src: "
        #version 450

        // Per shape input
        layout(set = 0, binding = 0) buffer GlobalData { int dummy; } globals;

        layout(set = 4, binding = 0) buffer ShapeTransforms { uint data[]; } shape_transforms;
        layout(set = 4, binding = 1) buffer ShapeFlags { uint data[]; } shape_flags;
        layout(set = 4, binding = 2) buffer ShapeXformIndexes { float data[]; } shape_xform_indexes;

        void main() {
        }"
    }
}

mod test_fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
    src: "
        #version 450

        layout(set = 3, binding = 0) uniform sampler2DArray mega_atlas;

        void main() {
        }"
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;
    use pal::Palette;
    use shape_chunk::{DrawSelection, OpenChunk};
    use specs::prelude::*;
    use vulkano::buffer::CpuAccessibleBuffer;
    use window::{GraphicsConfigBuilder, GraphicsWindow};

    fn build_pipeline(
        window: &GraphicsWindow,
    ) -> Fallible<Arc<dyn GraphicsPipelineAbstract + Send + Sync>> {
        let vert_shader = test_vs::Shader::load(window.device())?;
        let frag_shader = test_fs::Shader::load(window.device())?;
        Ok(Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(vert_shader.main_entry_point(), ())
                .triangle_list()
                .cull_mode_back()
                .front_face_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(frag_shader.main_entry_point(), ())
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
        ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>)
    }

    fn base_descriptors(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<[Arc<dyn DescriptorSet + Send + Sync>; 3]> {
        let global0 = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::Global.into())
                .add_buffer(CpuAccessibleBuffer::from_data(
                    window.device(),
                    BufferUsage::all(),
                    0u32,
                )?)?
                .build()?,
        );
        let empty1 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 1)?;
        let empty2 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 2)?;
        Ok([global0, empty1, empty2])
    }

    #[test]
    fn test_creation() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let lib = omni.library("FA");
        let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let pipeline = build_pipeline(&window)?;

        let mut inst_man = ShapeInstanceManager::new(
            pipeline.clone(),
            base_descriptors(pipeline.clone(), &window)?,
            &window,
        )?;

        let mut world = World::new();
        world.register::<ShapeComponent>();

        for _ in 0..100 {
            let slot_id = inst_man.upload_and_allocate_slot(
                "T80.SH",
                DrawSelection::NormalModel,
                &palette,
                &lib,
                &window,
            )?;
            let _ent = world
                .create_entity()
                .with(ShapeComponent { slot_id })
                .build();
        }

        Ok(())
    }
}
