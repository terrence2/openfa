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
use failure::{bail, ensure, Fallible};
use nalgebra::Matrix4;
use nalgebra::Point3;
use omnilib::OmniLib;
use shape_chunk::{Chunk, ClosedChunk, DrawSelection, OpenChunk, ShapeId, Vertex};
use specs::{world::Index, DispatcherBuilder, Entities, Join, ReadStorage, System, VecStorage};
use std::{collections::HashMap, mem, sync::Arc};
use vulkano::{
    buffer::{CpuAccessibleBuffer, CpuBufferPool},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DrawIndirectCommand},
    framebuffer::Subpass,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
};
use window::GraphicsWindow;
use world::{
    component::{ShapeMesh, Transform},
    Entity, World,
};

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
    include: ["./libs/render"],
    src: "
        #version 450
        #include <common/include/include_global.glsl>
        #include <buffer/shape_chunk/src/include_shape.glsl>

        // Scene info
        layout(push_constant) uniform PushConstantData {
            mat4 view;
            mat4 projection;
        } pc;

        // Per shape input
        const uint MAX_XFORM_ID = 32;
        layout(set = 3, binding = 0) buffer ChunkBaseTransforms {
            float data[];
        } shape_transforms;
        layout(set = 3, binding = 1) buffer ChunkFlags {
            uint data[];
        } shape_flags;
        layout(set = 3, binding = 2) buffer ChunkXforms {
            float data[];
        } shape_xforms;
        layout(set = 3, binding = 3) buffer ChunkXformOffsets {
            uint data[];
        } shape_xform_offsets;

        // Per Vertex input
        layout(location = 0) in vec3 position;
        layout(location = 1) in vec4 color;
        layout(location = 2) in vec2 tex_coord;
        layout(location = 3) in uint flags0;
        layout(location = 4) in uint flags1;
        layout(location = 5) in uint xform_id;

        layout(location = 0) smooth out vec4 v_color;
        layout(location = 1) smooth out vec2 v_tex_coord;
        layout(location = 2) flat out uint f_flags0;
        layout(location = 3) flat out uint f_flags1;

        void main() {
            uint base_transform = gl_InstanceIndex * 6;
            uint base_flag = gl_InstanceIndex * 2;
            uint base_xform = shape_xform_offsets.data[gl_InstanceIndex];

            float transform[6] = {
                shape_transforms.data[base_transform + 0],
                shape_transforms.data[base_transform + 1],
                shape_transforms.data[base_transform + 2],
                shape_transforms.data[base_transform + 3],
                shape_transforms.data[base_transform + 4],
                shape_transforms.data[base_transform + 5]
            };
            float xform[6] = {0, 0, 0, 0, 0, 0};
            if (xform_id < MAX_XFORM_ID) {
                xform[0] = shape_xforms.data[base_xform + 6 * xform_id + 0];
                xform[1] = shape_xforms.data[base_xform + 6 * xform_id + 1];
                xform[2] = shape_xforms.data[base_xform + 6 * xform_id + 2];
                xform[3] = shape_xforms.data[base_xform + 6 * xform_id + 3];
                xform[4] = shape_xforms.data[base_xform + 6 * xform_id + 4];
                xform[5] = shape_xforms.data[base_xform + 6 * xform_id + 5];
            }

            gl_Position = pc.projection * pc.view * matrix_for_xform(transform) * matrix_for_xform(xform) * vec4(position, 1.0);
            v_color = color;
            v_tex_coord = tex_coord;

            f_flags0 = flags0 & shape_flags.data[base_flag + 0];
            f_flags1 = flags1 & shape_flags.data[base_flag + 1];
        }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
    include: ["./libs/render"],
    src: "
        #version 450

        layout(location = 0) smooth in vec4 v_color;
        layout(location = 1) smooth in vec2 v_tex_coord;
        layout(location = 2) flat in uint f_flags0;
        layout(location = 3) flat in uint f_flags1;

        layout(location = 0) out vec4 f_color;

        layout(set = 4, binding = 0) uniform sampler2DArray mega_atlas;
        //layout(set = 5, binding = 1) uniform sampler2DArray nose_art; NOSE\\d\\d.PIC
        //layout(set = 5, binding = 2) uniform sampler2DArray left_tail_art; LEFT\\d\\d.PIC
        //layout(set = 5, binding = 3) uniform sampler2DArray right_tail_art; RIGHT\\d\\d.PIC
        //layout(set = 5, binding = 4) uniform sampler2DArray round_art; ROUND\\d\\d.PIC

        void main() {
            if ((f_flags0 & 0xFFFFFFFE) == 0 && f_flags1 == 0) {
                discard;
            } else if (v_tex_coord.x == 0.0) {
                f_color = v_color;
            } else {
                vec4 tex_color = texture(mega_atlas, vec3(v_tex_coord, 0));

                if ((f_flags0 & 1) == 1) {
                    f_color = vec4((1.0 - tex_color[3]) * v_color.xyz + tex_color[3] * tex_color.xyz, 1.0);
                } else {
                    if (tex_color.a < 0.5)
                        discard;
                    else
                        f_color = tex_color;
                }
            }
        }"
    }
}
const BLOCK_SIZE: usize = 128;

pub struct ShapeChunkManager {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,

    open_mover_chunk: OpenChunk,
    mover_chunks: Vec<ClosedChunk>,
}

impl ShapeChunkManager {
    pub fn new(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        Ok(Self {
            pipeline,
            open_mover_chunk: OpenChunk::new(window)?,
            mover_chunks: Vec::new(),
        })
    }

    // pub fn create_building

    // pub fn create_airplane -- need to hook into shape state?

    pub fn upload_mover(
        &mut self,
        name: &str,
        selection: DrawSelection,
        world: &World,
        window: &GraphicsWindow,
    ) -> Fallible<ShapeId> {
        if self.open_mover_chunk.chunk_is_full() {
            let mut open_chunk = OpenChunk::new(window)?;
            mem::swap(&mut open_chunk, &mut self.open_mover_chunk);
            //self.mover_chunks.push(ClosedChunk::new(open_chunk, pipeline, window))
        }
        let shape_id = self.open_mover_chunk.upload_shape(
            name,
            selection,
            world.system_palette(),
            world.library(),
            window,
        )?;
        Ok(shape_id)
    }
}

/*
// Combines a single shape chunk with a collection of instance blocks.
//
// We are uploading data on every frame, so we need fixed sized upload pools.
// Each pool can only handle so many instances though, so we may need more than
// one block of pools to service every instance that needs vertices in a chunk.
pub struct ChunkInstances {
    chunk: Chunk,

    // FIXME: we probably want to store these as traits so that we can have
    // FIXME: blocks with different upload characteristics.
    blocks: Vec<DynamicInstanceBlock>,
}
*/

// Fixed reservation blocks for upload of a number of entities. Unfortunately, because of
// xforms, we don't know exactly how many instances will fit in any given block.
pub struct DynamicInstanceBlock {
    // Weak reference to the associated chunk in the Manager.
    chunk_offset: usize,
    //chunk_type: ChunkType,

    // Map from the entity to the stored offset and from the offset to the entity.
    slot_map: HashMap<Index, usize>,
    reverse_slot_map: HashMap<usize, Index>,

    // Buffers for all instances stored in this instance set. One command per unique entity.
    // 16 bytes per entity; index unnecessary for draw
    command_buf: CpuBufferPool<[DrawIndirectCommand; BLOCK_SIZE]>,

    // Base position and orientation in xyz+euler angles stored as 6 adjacent floats.
    // 24 bytes per entity; buffer index inferable from drawing index
    base_buffer: CpuBufferPool<[[f32; 6]; BLOCK_SIZE]>, // Flags buffers

    // 2 32bit flags words for each entity.
    // 8 bytes per entity; buffer index inferable from drawing index
    flags_buffer: CpuBufferPool<[[u32; 2]; BLOCK_SIZE]>,

    // 0 to 14 position/orientation [f32; 6], depending on the shape.
    // assume 96 bytes per entity if we're talking about planes
    // cannot infer position, so needs an index buffer
    xform_buffer: CpuBufferPool<[[f32; 6]; 4 * BLOCK_SIZE]>,

    // 4 bytes per entity; can infer position from index
    xform_index_buffer: CpuBufferPool<[i32; BLOCK_SIZE]>,
}

impl DynamicInstanceBlock {}

struct ShapeRenderSystem {
    chunks: ShapeChunkManager,

    // All upload blocks. We will do one draw call per instance block each frame.
    blocks: Vec<DynamicInstanceBlock>,

    // Map from the index to the block that it has a reserved upload slot in.
    upload_block_map: HashMap<Index, usize>,
}

impl ShapeRenderSystem {
    pub fn new(chunks: ShapeChunkManager) -> Self {
        Self {
            chunks,
            blocks: Vec::new(),
            upload_block_map: HashMap::new(),
        }
    }

    pub fn build_pipeline(
        window: &GraphicsWindow,
    ) -> Fallible<Arc<dyn GraphicsPipelineAbstract + Send + Sync>> {
        let vert_shader = vs::Shader::load(window.device())?;
        let frag_shader = fs::Shader::load(window.device())?;
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

    // First fit: find the first block with a free upload slot.
    fn reserve_free_slot(&mut self) -> (usize, usize) {
        for (block_index, block) in self.blocks.iter().enumerate() {
            if let Some(slot_index) = block.reserve_free_slot(id) {
                return (block_index, slot_index);
            }
        }

        // No free slots in any blocks. Build a new one.
        let block_index = self.blocks.len();
        self.blocks.push(DynamicInstanceBlock::new());
        let slot_index = self.blocks.last().unwrap().reserve_free_slot(id).unwrap();
        return (block_index, slot_index);
    }

    pub fn reserve_entity_slot(&mut self, id: Index) -> Fallible<()> {
        if self.upload_slots.contains_key(&id) {
            return Ok(());
        }
        let (block_index, slot_index) = self.reserve_free_slot();
    }
}

impl<'a> System<'a> for ShapeRenderSystem {
    // These are the resources required for execution.
    // You can also define a struct and `#[derive(SystemData)]`,
    // see the `full` example.
    type SystemData = (
        Entities<'a>,
        ReadStorage<'a, Transform>,
        ReadStorage<'a, ShapeMesh>,
    );

    fn run(&mut self, (entities, transform, shape_mesh): Self::SystemData) {
        for (entity, transform, shape_mesh) in (&entities, &transform, &shape_mesh).join() {
            self.reserve_entity_slot(entity.id());
            // Check if entity has been uploaded.
            //   if not, then allocate slot and add map to mapping
            // Update slot with new values from transform, flags, and xforms
            // after all updates, push all buffer pools?
            println!("{:?} => shape_id: {:?}", entity.id(), shape_mesh.shape_id());
            //self.push_mesh(transform, shape_mesh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkano::pipeline::GraphicsPipeline;
    use window::GraphicsConfigBuilder;
    use world::World;

    #[test]
    fn it_works() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;

        let window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let lib = omni.library("FA");

        let mut world = World::new(lib)?;

        let pipeline = ShapeRenderSystem::build_pipeline(&window)?;
        let mut shape_chunk_man = ShapeChunkManager::new(pipeline, &window)?;
        let t80_id =
            shape_chunk_man.upload_mover("T80.SH", DrawSelection::NormalModel, &world, &window)?;

        let t80_ent1 = world.create_ground_mover(t80_id, Point3::new(0f64, 0f64, 0f64))?;

        let mut dispatcher = DispatcherBuilder::new()
            .with(ShapeRenderSystem::new(shape_chunk_man), "", &[])
            .build();
        world.run(&mut dispatcher);
        let t80_ent2 = world.create_ground_mover(t80_id, Point3::new(0f64, 0f64, 0f64))?;
        world.run(&mut dispatcher);
        let t80_ent3 = world.create_ground_mover(t80_id, Point3::new(0f64, 0f64, 0f64))?;
        world.run(&mut dispatcher);

        world.destroy_entity(t80_ent2)?;
        world.run(&mut dispatcher);
        world.destroy_entity(t80_ent1)?;
        world.run(&mut dispatcher);

        Ok(())
    }
}

/*
// Types of data we want to be able to deal with.
//
// Static Immortal:
//   CommandBuf: [ Name1(0...N), Name2(0...M), ...]
//   BaseBuffer: [A, A, A, ... A{N}, B, B, B, ... B{M}]; A/B: [f32; 6]
//   FlagsBuffer: []
//   XFormBuffer: []
//
// We need to accumulate before uploading the command buffer, which means we need to be
// careful with the order in BaseBuffer. Assert that there are no xforms or flags on any of these.
// How much can we simplify the renderer if we know there are no xforms?
//
// Xforms vs no xforms -- most shapes have no xforms, even if they can be destroyed, or
// move around and be destroyed. How much can we simplify the renderer if we don't have
// xforms? Probably quite a bit. Is it worth having two pipelines? Benchmark to figure out
// how many fully dynamic shapes we can have.
//
// Fully dynamic:
//   CommandBuf: [ E0, E1, E2, E3, ... EN ]  <- updated on add/remove entity (as are all)
//   BaseBuffer: [ B0, B1, B2, B3, ... BN ]  <- updated every frame for movers, never for static
//   FlagsBuffer: [ F0, F1, F2, F3, ... FN ] <- updated occasionally
//   XformBuffer: [ X0..M, X0...L, X0...H ... X0...I ] <- updated every frame for some things
//
// Implement fullest feature set first. If we can render a million SOLDIER.SH, we can easily
// render a million TREE.SH.

pub struct OpenChunkInstance {
    open_chunk: OpenChunk,
    command_buf: Vec<Entity>,
    base_buffer: Vec<Matrix4<f32>>,
    flags_buffer: Vec<[u32; 2]>,
}

pub struct InstanceSet {
    // Offset of the chunk these instances draw from.
    chunk_reference: usize,

    // Buffers for all instances stored in this instance set. One command per unique entity.
    // 16 bytes per entity; index unnecessary for draw
    command_buf: CpuAccessibleBuffer<[DrawIndirectCommand]>,

    // Base position and orientation in xyz+euler angles stored as 6 adjacent floats.
    // 24 bytes per entity; buffer index inferable from drawing index
    base_buffer: CpuAccessibleBuffer<[f32]>, // Flags buffers

    // 2 32bit flags words for each entity.
    // 8 bytes per entity; buffer index inferable from drawing index
    flags_buffer: CpuAccessibleBuffer<[u32]>,

    // 0 to 14 position/orientation [f32; 6], depending on the shape.
    // assume 240 bytes per entity if we're talking about planes
    // cannot infer position, so needs an index buffer
    xform_buffer: CpuAccessibleBuffer<[f32]>,

    // 4 bytes per entity; can infer position from index
    xform_index_buffer: CpuAccessibleBuffer<[i32]>,
    //
    // Total cost per entity is: 16 + 24 + 8 + 240 + 4 ~ 300 bytes per entity
    // We cannot really upload more than 1MiB per frame, so... ~3000 planes
}
*/
