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
use camera::CameraAbstract;
use failure::Fallible;
use global_layout::GlobalSets;
use lib::Library;
use nalgebra::Matrix4;
use pal::Palette;
use shape_chunk::{DrawSelection, Vertex};
use shape_instance::ShapeInstanceManager;
use std::{sync::Arc, time::Instant};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::AutoCommandBufferBuilder,
    descriptor::descriptor_set::PersistentDescriptorSet,
    framebuffer::Subpass,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
    sync::GpuFuture,
};
use window::GraphicsWindow;

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
        layout(set = 0, binding = 0) buffer GlobalData {
            int dummy;
        } globals;

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
            gl_Position = pc.projection *
                          pc.view *
                          matrix_for_xform(transform) *
                          matrix_for_xform(xform) *
                          vec4(position, 1.0);
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

        layout(set = 5, binding = 0) uniform sampler2DArray mega_atlas;
        //layout(set = 6, binding = 1) uniform sampler2DArray nose_art; NOSE\\d\\d.PIC
        //layout(set = 6, binding = 2) uniform sampler2DArray left_tail_art; LEFT\\d\\d.PIC
        //layout(set = 6, binding = 3) uniform sampler2DArray right_tail_art; RIGHT\\d\\d.PIC
        //layout(set = 6, binding = 4) uniform sampler2DArray round_art; ROUND\\d\\d.PIC

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
}

pub struct ShapeRenderer {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    push_consts: vs::ty::PushConstantData,
    inst_man: ShapeInstanceManager,

    #[allow(dead_code)]
    start_time: Instant,
}

impl ShapeRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        let pipeline = Self::build_pipeline(&window)?;

        //let empty0 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 0)?;
        let globals_buffer =
            CpuAccessibleBuffer::from_data(window.device(), BufferUsage::all(), 0u32)?;
        let global0 = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), GlobalSets::Global.into())
                .add_buffer(globals_buffer)?
                .build()?,
        );
        let empty = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 1)?;

        let base_descriptors = [
            global0,
            empty.clone(),
            empty.clone(),
            empty.clone(),
            empty.clone(),
            empty.clone(),
        ];
        let inst_man = ShapeInstanceManager::new(pipeline.clone(), base_descriptors, &window)?;

        Ok(Self {
            pipeline,
            push_consts: vs::ty::PushConstantData::new(),
            start_time: Instant::now(),
            inst_man,
        })
    }

    fn build_pipeline(
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

    pub fn pipeline(&self) -> Arc<dyn GraphicsPipelineAbstract + Send + Sync> {
        self.pipeline.clone()
    }

    pub fn upload_shape(
        &mut self,
        name: &str,
        selection: DrawSelection,
        palette: &Palette,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<Option<Box<dyn GpuFuture>>> {
        let (_chunk_id, _slot_id, future) = self
            .inst_man
            .upload_and_allocate_slot(name, selection, palette, lib, window)?;
        Ok(future)
    }

    // Close any outstanding chunks and prepare to render.
    pub fn ensure_uploaded(
        &mut self,
        window: &GraphicsWindow,
    ) -> Fallible<Option<Box<dyn GpuFuture>>> {
        self.inst_man.ensure_uploaded(window)
    }

    /*
    fn allocate_entity_slot(
        &mut self,
        id: EntityId,
        shape_id: ShapeId,
        draw_command: DrawIndirectCommand,
    ) -> Fallible<()> {
        assert!(!self.knows_entity(id));

        let chunk = self.chunks.get_chunk_for_shape(shape_id);

        // Note that we do not bother sorting blocks by chunk because we only have to care about
        // that mapping when adding new entries. We do a simple chunk_id check to filter out
        // non-matching blocks. The assumption is that we will have few enough chunks that a large
        // fraction of blocks will be relevant, usually.
        for (block_index, block) in self.blocks.iter_mut() {
            if let Some(_) = block.allocate_entity_slot(id, chunk.index(), draw_command) {
                self.upload_block_map.insert(id, *block_index);
                return Ok(());
            }
        }

        // No free slots in any blocks. Build a new one.
        let next_block_index = BlockIndex(self.next_block_index);
        self.next_block_index += 1;
        let mut block = DynamicInstanceBlock::new(
            chunk.index(),
            self.pipeline.clone(),
            &self.base_descriptors,
            self.command_buffer_pool.clone(),
            self.transform_buffer_pool.clone(),
            self.flag_buffer_pool.clone(),
            self.xform_index_buffer_pool.clone(),
            self.xform_buffer_pool.clone(),
            self.device.clone(),
        )?;
        block
            .allocate_entity_slot(id, chunk.index(), draw_command)
            .unwrap();
        self.blocks.insert(next_block_index, block);
        self.upload_block_map.insert(id, next_block_index);
        Ok(())
    }

    fn get_entity_block_mut(&mut self, id: EntityId) -> Option<&mut DynamicInstanceBlock> {
        let block_index = &self.upload_block_map[&id];
        self.blocks.get_mut(block_index)
    }

    fn get_entity_block(&self, id: EntityId) -> &DynamicInstanceBlock {
        &self.blocks[&self.upload_block_map[&id]]
    }

    pub fn chunks(&self) -> &ShapeChunkManager {
        &self.chunks
    }

    fn get_chunk_for_block(&self, block: &DynamicInstanceBlock) -> &ClosedChunk {
        self.chunks.get_chunk(block.chunk_index)
    }
    */

    pub fn update_buffers(
        &mut self,
        cbb: AutoCommandBufferBuilder,
    ) -> Fallible<AutoCommandBufferBuilder> {
        self.inst_man.upload_buffers(cbb)
    }

    pub fn render(
        &mut self,
        cbb: AutoCommandBufferBuilder,
        camera: &dyn CameraAbstract,
        window: &GraphicsWindow,
    ) -> Fallible<AutoCommandBufferBuilder> {
        self.push_consts.set_projection(&camera.projection_matrix());
        self.push_consts.set_view(&camera.view_matrix());
        self.inst_man
            .render(cbb, &window.dynamic_state, &self.push_consts)
    }

    /*
    pub fn maintain(&mut self) {
        let mut finished_blocks = Vec::new();
        let mut removals = Vec::new();
        for (block_index, block) in self.blocks.iter_mut() {
            if block.maintain(&mut removals) {
                finished_blocks.push(*block_index);
            }
        }
        for removal in &removals {
            self.upload_block_map.remove(removal);
        }
        for finished in &finished_blocks {
            self.blocks.remove(finished);
        }
    }

    fn update_entity(
        &mut self,
        now: &Instant,
        id: EntityId,
        transform: &Transform,
        shape_mesh: &ShapeMesh,
    ) -> Fallible<()> {
        let start = self.start_time;

        if !self.knows_entity(id) {
            let chunk_part = self.chunks.part(shape_mesh.shape_id());
            let draw_command = chunk_part.draw_command(0, 1);
            self.allocate_entity_slot(id, shape_mesh.shape_id(), draw_command)
                .expect("unable to reserve instance slot");
        }

        let chunk_part = self.chunks.part(shape_mesh.shape_id())?;
        let errata = chunk_part.widgets().read().unwrap().errata();

        let next_transform = transform.compact();
        let mut next_flags = [0u32; 2];
        let mut next_xforms = [0f32; 6 * 14];
        let xform_value_count = chunk_part.widgets().read().unwrap().num_transformer_floats();
        shape_mesh
            .draw_state()
            .build_mask_into(&start, errata, &mut next_flags)
            .expect("failed to build flags for shape");
        chunk_part
            .widgets()
            .write()
            .unwrap()
            .animate_into(shape_mesh.draw_state(), &start, now, &mut next_xforms)
            .expect("");

        let block = self.get_entity_block_mut(id).unwrap();
        let slot = block.get_existing_slot(id);
        *block.get_transform_buffer_slot(slot) = next_transform;
        *block.get_flag_buffer_slot(slot) = next_flags;
        block
            .get_xform_buffer_slot(slot)
            .copy_from_slice(&next_xforms[0..xform_value_count]);

        Ok(())
    }
    */
}
