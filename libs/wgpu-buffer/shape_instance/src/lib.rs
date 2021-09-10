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
mod instance_block;

pub use crate::instance_block::SlotId;
pub use components::*;
pub use shape_chunk::{DrawSelection, DrawState};

use crate::instance_block::{BlockId, InstanceBlock, TransformType};
use absolute_unit::Kilometers;
use anyhow::Result;
use atmosphere::AtmosphereBuffer;
use camera::Camera;
use catalog::Catalog;
use global_data::GlobalParametersBuffer;
use gpu::{Gpu, UploadTracker};
use legion::*;
use nalgebra::{convert, Matrix4, UnitQuaternion};
use ofa_groups::Group as LocalGroup;
use pal::Palette;
use parking_lot::RwLock;
use shader_shared::Group;
use shape_chunk::{
    ChunkId, ChunkPart, ShapeChunkBuffer, ShapeErrata, ShapeId, ShapeWidgets, Vertex,
};
use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
    time::Instant,
};
use universe::component::{Rotation, Scale, Transform};

thread_local! {
    pub static WIDGET_CACHE: RefCell<HashMap<ShapeId, ShapeWidgets>> = RefCell::new(HashMap::new());
}

pub struct ShapeInstanceBuffer {
    pub chunk_man: ShapeChunkBuffer,

    chunk_to_block_map: HashMap<ChunkId, Vec<BlockId>>,
    blocks: HashMap<BlockId, InstanceBlock>,
    next_block_id: u32,

    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl ShapeInstanceBuffer {
    pub fn new(
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        gpu: &Gpu,
    ) -> Result<Arc<RwLock<Self>>> {
        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("shape-instance-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::transform_buffer_size(),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::flag_buffer_size(),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::xform_index_buffer_size(),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::xform_buffer_size(),
                            },
                            count: None,
                        },
                    ],
                });

        let chunk_man = ShapeChunkBuffer::new(gpu.device())?;

        let vert_shader =
            gpu.create_shader_module("shape.vert", include_bytes!("../target/shape.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module("shape.frag", include_bytes!("../target/shape.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shape-render-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        chunk_man.bind_group_layout(),
                        &bind_group_layout,
                    ],
                });

        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("shape-render-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vert_shader,
                    entry_point: "main",
                    buffers: &[Vertex::descriptor()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &frag_shader,
                    entry_point: "main",
                    targets: &[wgpu::ColorTargetState {
                        format: Gpu::SCREEN_FORMAT,
                        color_blend: wgpu::BlendState::REPLACE,
                        alpha_blend: wgpu::BlendState::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::Back,
                    polygon_mode: wgpu::PolygonMode::Fill,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: Gpu::DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Greater,
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState::IGNORE,
                        back: wgpu::StencilFaceState::IGNORE,
                        read_mask: 0,
                        write_mask: 0,
                    },
                    bias: wgpu::DepthBiasState {
                        constant: 0,
                        slope_scale: 0.0,
                        clamp: 0.0,
                    },
                    clamp_depth: false,
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
            });

        Ok(Arc::new(RwLock::new(Self {
            chunk_man,
            chunk_to_block_map: HashMap::new(),
            blocks: HashMap::new(),
            next_block_id: 0,
            bind_group_layout,
            pipeline,
        })))
    }

    // pub fn block(&self, id: &BlockId) -> &InstanceBlock {
    //     &self.blocks[id]
    // }

    pub fn part(&self, shape_id: ShapeId) -> &ChunkPart {
        self.chunk_man.part(shape_id)
    }

    pub fn errata(&self, shape_id: ShapeId) -> ShapeErrata {
        self.chunk_man.part(shape_id).widgets().read().errata()
    }

    fn allocate_block_id(&mut self) -> BlockId {
        assert!(self.next_block_id < std::u32::MAX);
        let bid = self.next_block_id;
        self.next_block_id += 1;
        BlockId::new(bid)
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
        catalog: &Catalog,
        gpu: &mut Gpu,
    ) -> Result<(ShapeId, SlotId)> {
        // Ensure that the shape is actually in a chunk somewhere.
        let (chunk_id, shape_id) = self
            .chunk_man
            .upload_shape(name, selection, palette, catalog, gpu)?;

        // Find or create a block that we can use to track the instance data.
        let block_id = if let Some(block_id) = self.find_open_block(chunk_id) {
            block_id
        } else {
            let block_id = self.allocate_block_id();
            let block =
                InstanceBlock::new(block_id, chunk_id, &self.bind_group_layout, gpu.device());
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

    pub fn ensure_uploaded(&mut self, gpu: &mut Gpu) -> Result<()> {
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
        self.blocks
            .get_mut(slot_id.block_id())
            .unwrap()
            .push_values(slot_id, transform, flags, xforms, xform_count);
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn make_upload_buffer(
        &mut self,
        start: &Instant,
        now: &Instant,
        camera: &Camera,
        world: &mut World,
        gpu: &Gpu,
        tracker: &mut UploadTracker,
    ) -> Result<()> {
        // Reset cursor for our next upload.
        for block in self.blocks.values_mut() {
            block.begin_frame();
        }

        // Animate the draw_state. We'll use the updated values below when computing
        // xform and frame based animation states.
        <Write<ShapeState>>::query()
            .par_for_each_mut(world, |shape_state| shape_state.draw_state.animate(now));

        let km2m = Matrix4::new_scaling(1_000.0);
        let view = camera.view::<Kilometers>().to_homogeneous();
        let mut query = <(
            Read<Transform>,
            Read<Rotation>,
            Read<Scale>,
            Write<ShapeTransformBuffer>,
        )>::query();
        // TODO: distinguish first run, as it doesn't seem to see "new" as changed.
        //    .filter(changed::<Transform>() | changed::<Rotation>());
        query.par_for_each_mut(world, |(transform, rotation, scale, transform_buffer)| {
            // Transform must be performed in f64, then moved into view space (where precision
            // errors are at least far away), before being truncated to f32.
            let pos = transform.cartesian().point64().to_homogeneous();
            let pos_view = km2m * view * pos;
            transform_buffer.buffer[0] = pos_view.x as f32;
            transform_buffer.buffer[1] = pos_view.y as f32;
            transform_buffer.buffer[2] = pos_view.z as f32;

            // Since we are uploading with eye space rotations applied, we need to "undo"
            // the eye-space rotation before uploading so that we will be world aligned.
            let (a, b, c) = rotation.quaternion().euler_angles();
            transform_buffer.buffer[3] = a;
            transform_buffer.buffer[4] = b;
            transform_buffer.buffer[5] = c;

            transform_buffer.buffer[6] = scale.scale();
        });

        let mut query = <(Read<ShapeState>, Write<ShapeFlagBuffer>)>::query();
        query.par_for_each_mut(world, |(shape_state, flag_buffer)| {
            shape_state
                .draw_state
                .build_mask_into(start, &mut flag_buffer.buffer)
                .unwrap();
        });

        let mut query = <(Read<ShapeRef>, Read<ShapeState>, Write<ShapeXformBuffer>)>::query();
        query.par_for_each_mut(world, |(shape_ref, shape_state, xform_buffer)| {
            let part = self.chunk_man.part(shape_ref.shape_id);
            WIDGET_CACHE.with(|widget_cache| {
                match widget_cache.borrow_mut().entry(shape_ref.shape_id) {
                    Entry::Occupied(mut e) => {
                        e.get_mut()
                            .animate_into(
                                &shape_state.draw_state,
                                start,
                                now,
                                &mut xform_buffer.buffer,
                            )
                            .unwrap();
                    }
                    Entry::Vacant(e) => {
                        let mut widgets = part.widgets().read().clone();
                        widgets
                            .animate_into(
                                &shape_state.draw_state,
                                start,
                                now,
                                &mut xform_buffer.buffer,
                            )
                            .unwrap();
                        e.insert(widgets);
                    }
                }
            });
        });

        let mut query = <(
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
            block.make_upload_buffer(gpu, tracker);
        }
        Ok(())
    }

    pub fn draw_shapes<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
        atmosphere_buffer: &'a AtmosphereBuffer,
    ) -> Result<wgpu::RenderPass<'a>> {
        assert_ne!(LocalGroup::ShapeChunk.index(), Group::Globals.index());
        assert_ne!(LocalGroup::ShapeChunk.index(), Group::Atmosphere.index());
        assert_ne!(LocalGroup::ShapeBlock.index(), Group::Globals.index());
        assert_ne!(LocalGroup::ShapeBlock.index(), Group::Atmosphere.index());
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            atmosphere_buffer.bind_group(),
            &[],
        );

        for block in self.blocks.values() {
            let chunk = self.chunk_man.chunk(block.chunk_id());

            // FIXME: reorganize blocks by chunk so that we can avoid thrashing this bind group
            rpass.set_bind_group(LocalGroup::ShapeChunk.index(), chunk.bind_group(), &[]);
            rpass.set_bind_group(LocalGroup::ShapeBlock.index(), block.bind_group(), &[]);
            rpass.set_vertex_buffer(0, chunk.vertex_buffer());
            for i in 0..block.len() {
                //rpass.draw_indirect(block.command_buffer(), i as u64);
                let cmd = block.command_buffer_scratch[i];
                #[allow(clippy::range_plus_one)]
                rpass.draw(
                    cmd.first_vertex..cmd.first_vertex + cmd.vertex_count,
                    i as u32..i as u32 + 1,
                );
            }
        }
        Ok(rpass)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lib::CatalogBuilder;
    use nitrous::Interpreter;
    use pal::Palette;
    use shape_chunk::DrawSelection;
    use winit::{event_loop::EventLoop, window::Window};

    #[cfg(unix)]
    #[test]
    fn test_creation() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;

        let skipped = vec![
            "CATGUY.SH",  // 640
            "MOON.SH",    // 41
            "SOLDIER.SH", // 320
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

        let (mut catalog, inputs) = CatalogBuilder::build_and_select(&["*:*.SH".to_owned()])?;
        let mut shapes = HashMap::new();
        for &fid in &inputs {
            shapes
                .entry(catalog.file_label(fid).unwrap())
                .or_insert_with(Vec::new)
                .push(fid)
        }

        for (label, files) in &shapes {
            catalog.set_default_label(label);
            let game = label.split(':').last().unwrap();
            let palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;

            let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu.write())?;
            let globals_buffer =
                GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
            let inst_man = ShapeInstanceBuffer::new(
                &globals_buffer.read(),
                &atmosphere_buffer.read(),
                &gpu.read(),
            )?;
            let mut all_chunks = Vec::new();
            let mut all_slots = Vec::new();
            for &fid in files {
                let meta = catalog.stat_sync(fid)?;
                println!(
                    "At: {}:{:13} @ {}",
                    game,
                    meta.name(),
                    meta.path()
                        .map(|v| v.to_string_lossy())
                        .unwrap_or_else(|| "<none>".into())
                );
                let name = meta.name().to_owned();
                if skipped.contains(&meta.name()) {
                    continue;
                }

                for _ in 0..1 {
                    let (chunk_id, slot_id) = inst_man.write().upload_and_allocate_slot(
                        &name,
                        DrawSelection::NormalModel,
                        &palette,
                        &catalog,
                        &mut gpu.write(),
                    )?;
                    all_chunks.push(chunk_id);
                    all_slots.push(slot_id);
                }
                gpu.read().device().poll(wgpu::Maintain::Wait);
            }
        }

        Ok(())
    }
}
