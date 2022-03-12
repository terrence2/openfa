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
mod chunk;
pub mod component;
mod components;
mod instance_block;

pub use crate::{
    chunk::{DrawSelection, DrawState},
    instance_block::SlotId,
};
pub use components::*;

use crate::{
    chunk::{
        ChunkId, ChunkManager, ChunkPart, ShapeErrata, ShapeId, ShapeIds, ShapeWidgets, Vertex,
    },
    // component::Scale,
    instance_block::{BlockId, InstanceBlock, TransformType},
};
use absolute_unit::Meters;
use animate::TimeStep;
use anyhow::Result;
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use bevy_tasks::TaskPool;
use camera::ScreenCamera;
use catalog::Catalog;
use composite::CompositeRenderStep;
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use measure::WorldSpaceFrame;
use nitrous::NamedEntityMut;
use ofa_groups::Group as LocalGroup;
use pal::Palette;
use runtime::{Extension, FrameStage, Runtime};
use sh::RawShape;
use shader_shared::Group;
use smallvec::SmallVec;
use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    time::Instant,
};
use world::{WorldRenderPass, WorldRenderStep};

thread_local! {
    pub static WIDGET_CACHE: RefCell<HashMap<ShapeId, ShapeWidgets>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
enum ChunkUpload {}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum ShapeUploadStep {
    ResetUploadCursor,
    AnimateDrawState,
    ApplyTransforms,
    ApplyFlags,
    ApplyXforms,
    PushToBlock,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum ShapeRenderStep {
    UploadChunks,
    UploadBlocks,
    Render,
}

pub struct ShapeInstance {
    pub shape_id: ShapeId,
    pub slot_id: SlotId,
}

pub struct ShapeComponents {
    pub draw_state: DrawState,
    pub transform_buffer: ShapeTransformBuffer,
    pub flag_buffer: ShapeFlagBuffer,
    pub xform_buffer: ShapeXformBuffer,
}

#[derive(Debug)]
pub struct ShapeBuffer {
    chunk_man: ChunkManager,

    chunk_to_block_map: HashMap<ChunkId, Vec<BlockId>>,
    blocks: HashMap<BlockId, InstanceBlock>,
    next_block_id: u32,

    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl Extension for ShapeBuffer {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let shapes = ShapeBuffer::new(
            runtime.resource::<GlobalParametersBuffer>(),
            runtime.resource::<AtmosphereBuffer>(),
            runtime.resource::<Gpu>(),
        )?;

        runtime
            .frame_stage_mut(FrameStage::TrackStateChanges)
            .add_system(Self::sys_ts_reset_upload_cursor.label(ShapeUploadStep::ResetUploadCursor));
        runtime
            .frame_stage_mut(FrameStage::TrackStateChanges)
            .add_system(Self::sys_ts_animate_draw_state.label(ShapeUploadStep::AnimateDrawState));
        runtime
            .frame_stage_mut(FrameStage::TrackStateChanges)
            .add_system(Self::sys_ts_apply_transforms.label(ShapeUploadStep::ApplyTransforms));
        runtime
            .frame_stage_mut(FrameStage::TrackStateChanges)
            .add_system(Self::sys_ts_build_flag_mask.label(ShapeUploadStep::ApplyFlags));
        runtime
            .frame_stage_mut(FrameStage::TrackStateChanges)
            .add_system(
                Self::sys_ts_apply_xforms
                    .label(ShapeUploadStep::ApplyXforms)
                    .after(ShapeUploadStep::ResetUploadCursor),
            );
        runtime
            .frame_stage_mut(FrameStage::TrackStateChanges)
            .add_system(
                Self::sys_ts_push_values_to_block
                    .label(ShapeUploadStep::PushToBlock)
                    .after(ShapeUploadStep::ApplyTransforms)
                    .after(ShapeUploadStep::ApplyFlags)
                    .after(ShapeUploadStep::ApplyXforms),
            );

        runtime
            .frame_stage_mut(FrameStage::Render)
            .add_system(Self::sys_close_open_chunks.label(ShapeRenderStep::UploadChunks));
        runtime
            .frame_stage_mut(FrameStage::Render)
            .add_system(Self::sys_upload_block_frame_data.label(ShapeRenderStep::UploadBlocks));
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_draw_shapes
                .label(ShapeRenderStep::Render)
                .after(ShapeRenderStep::UploadChunks)
                .after(ShapeRenderStep::UploadBlocks)
                .after(WorldRenderStep::Render)
                .before(CompositeRenderStep::Render),
        );

        runtime
            .frame_stage_mut(FrameStage::FrameEnd)
            .add_system(Self::sys_cleanup_open_chunks_after_render);
        runtime.insert_resource(shapes);
        Ok(())
    }
}

impl ShapeBuffer {
    pub fn new(
        globals: &GlobalParametersBuffer,
        atmosphere: &AtmosphereBuffer,
        gpu: &Gpu,
    ) -> Result<Self> {
        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("shape-instance-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::transform_buffer_size(),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::flag_buffer_size(),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::xform_index_buffer_size(),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: InstanceBlock::xform_buffer_size(),
                            },
                            count: None,
                        },
                    ],
                });

        let chunk_man = ChunkManager::new(gpu)?;

        let vert_shader =
            gpu.create_shader_module("shape.vert", include_bytes!("../target/shape.vert.spirv"));
        let frag_shader =
            gpu.create_shader_module("shape.frag", include_bytes!("../target/shape.frag.spirv"));

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shape-render-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals.bind_group_layout(),
                        atmosphere.bind_group_layout(),
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
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: Some(wgpu::Face::Back),
                    unclipped_depth: true,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
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
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
            });

        Ok(Self {
            chunk_man,
            chunk_to_block_map: HashMap::new(),
            blocks: HashMap::new(),
            next_block_id: 0,
            bind_group_layout,
            pipeline,
        })
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
        assert!(self.next_block_id < u32::MAX);
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

    pub fn upload_shapes<S: AsRef<str>>(
        &mut self,
        palette: &Palette,
        shape_file_names: &[S],
        catalog: &Catalog,
        gpu: &Gpu,
    ) -> Result<HashMap<String, ShapeIds>> {
        let mut out = HashMap::new();

        // Load all SH files, including associated damage models (but not shadow shapes, those have
        // their own section in the OT files, for some reason).
        let mut shapes = HashMap::new();
        for shape_file_name in shape_file_names {
            let shape_file_name = shape_file_name.as_ref();
            shapes.insert(
                shape_file_name.to_owned(),
                RawShape::from_bytes(catalog.read_name(shape_file_name)?.as_ref())?,
            );
            let (base_name, _sh) = shape_file_name.rsplit_once('.').unwrap();
            for suffix in ["_A", "_B", "_C", "_D"] {
                let assoc_name = format!("{}{}.SH", base_name, suffix);
                if let Ok(data) = catalog.read_name(&assoc_name) {
                    shapes.insert(assoc_name, RawShape::from_bytes(data.as_ref())?);
                }
            }
        }

        // Upload all of the SH files we found above.
        let upload_results = self
            .chunk_man
            .upload_shapes(palette, &shapes, catalog, gpu)?;

        // Re-visit our shape_file_names and accumulate our uploaded shape data into useful results.
        for shape_file_name in shape_file_names {
            let shape_file_name = shape_file_name.as_ref();
            assert!(
                upload_results.contains_key(shape_file_name),
                "did not find expected loaded models"
            );
            let normal_shape_id = *upload_results
                .get(shape_file_name)
                .unwrap()
                .get(&DrawSelection::NormalModel)
                .expect("no normal model for base shape");

            let mut damage_shape_ids = SmallVec::new();
            if let Some(damage_shape_id) = upload_results
                .get(shape_file_name)
                .unwrap()
                .get(&DrawSelection::NormalModel)
            {
                damage_shape_ids.push(*damage_shape_id);
            }

            let (base_name, _sh) = shape_file_name.rsplit_once('.').unwrap();
            for suffix in ["_A", "_B", "_C", "_D"] {
                let assoc_name = format!("{}{}.SH", base_name, suffix);
                if let Some(damage_models) = upload_results.get(&assoc_name) {
                    let damage_shape_id = damage_models
                        .get(&DrawSelection::NormalModel)
                        .expect("separated damage models must have a normal model");
                    damage_shape_ids.push(*damage_shape_id);
                }
            }

            out.insert(
                shape_file_name.to_owned(),
                ShapeIds::new(normal_shape_id, damage_shape_ids),
            );
        }

        Ok(out)
    }

    fn sys_close_open_chunks(
        mut shapes: ResMut<ShapeBuffer>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            shapes.chunk_man.close_open_chunks(&gpu, encoder)
        }
    }

    fn sys_cleanup_open_chunks_after_render(mut shapes: ResMut<ShapeBuffer>, mut gpu: ResMut<Gpu>) {
        shapes.chunk_man.cleanup_open_chunks_after_render(&mut gpu);
    }

    pub fn instantiate(
        &mut self,
        mut entity: NamedEntityMut,
        shape_id: ShapeId,
        gpu: &Gpu,
    ) -> Result<()> {
        // Find or create a block that we can use to track the instance data.
        let chunk_id = self.chunk_man.shape_chunk(shape_id);
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
        let draw_cmd = self
            .chunk_man
            .part(shape_id)
            .draw_command(self.blocks[&block_id].len() as u32, 1);
        let slot_id = self
            .blocks
            .get_mut(&block_id)
            .unwrap()
            .allocate_slot(draw_cmd);
        let widgets = self.chunk_man.part(shape_id).widgets();
        let errata: ShapeErrata = widgets.read().errata();

        entity.insert(shape_id);
        entity.insert(slot_id);
        entity.insert_named(DrawState::new(errata))?;
        entity.insert(ShapeTransformBuffer::default());
        entity.insert(ShapeFlagBuffer::default());
        entity.insert(ShapeXformBuffer::default());

        Ok(())

        // Ok((
        //     ShapeInstance { shape_id, slot_id },
        //     ShapeComponents {
        //         draw_state: DrawState::new(errata),
        //         transform_buffer: ShapeTransformBuffer::default(),
        //         flag_buffer: ShapeFlagBuffer::default(),
        //         xform_buffer: ShapeXformBuffer::default(),
        //     },
        // ))
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

    pub fn sys_ts_reset_upload_cursor(mut shapes: ResMut<ShapeBuffer>) {
        // Reset cursor for this frame's uploads
        // FIXME: can we do this after upload so we don't have two serial phases?
        for block in shapes.blocks.values_mut() {
            block.begin_frame();
        }
    }

    pub fn sys_ts_animate_draw_state(step: Res<TimeStep>, mut query: Query<&mut DrawState>) {
        let now = step.now();
        for mut draw_state in query.iter_mut() {
            draw_state.animate(now);
        }
    }

    pub fn sys_ts_apply_transforms(
        task_pool: Res<TaskPool>,
        camera: Res<ScreenCamera>,
        mut query: Query<(&WorldSpaceFrame, &mut ShapeTransformBuffer)>,
    ) {
        let view = camera.view::<Meters>().to_homogeneous();
        query.par_for_each_mut(&task_pool, 1024, |(frame, mut transform_buffer)| {
            // Transform must be performed in f64, then moved into view space (where precision
            // errors are at least far away), before being truncated to f32.
            let pos = frame.position().point64().to_homogeneous();
            let pos_view = view * pos;
            transform_buffer.buffer[0] = pos_view.x as f32;
            transform_buffer.buffer[1] = pos_view.y as f32;
            transform_buffer.buffer[2] = pos_view.z as f32;

            // Since we are uploading with eye space rotations applied, we need to "undo"
            // the eye-space rotation before uploading so that we will be world aligned.
            // let fwd = frame.forward();
            let (a, b, c) = frame.facing32().euler_angles();
            transform_buffer.buffer[3] = a;
            transform_buffer.buffer[4] = b;
            transform_buffer.buffer[5] = c;

            // transform_buffer.buffer[6] = scale.scale();
            // FIXME: scaling
            transform_buffer.buffer[6] = 4.0;
        });
    }

    pub fn sys_ts_build_flag_mask(
        task_pool: Res<TaskPool>,
        step: Res<TimeStep>,
        mut query: Query<(&DrawState, &mut ShapeFlagBuffer)>,
    ) {
        let start = step.start();
        query.par_for_each_mut(&task_pool, 1024, |(draw_state, mut flag_buffer)| {
            draw_state
                .build_mask_into(start, &mut flag_buffer.buffer)
                .unwrap();
        });
    }

    pub fn sys_ts_apply_xforms(
        shapes: Res<ShapeBuffer>,
        task_pool: Res<TaskPool>,
        step: Res<TimeStep>,
        mut query: Query<(&ShapeId, &DrawState, &mut ShapeXformBuffer)>,
    ) {
        let start = step.start();
        let now = step.now();
        assert!(now >= start);
        query.par_for_each_mut(
            &task_pool,
            1024,
            |(shape_id, draw_state, mut xform_buffer)| {
                let part = shapes.chunk_man.part(*shape_id);
                WIDGET_CACHE.with(|widget_cache| {
                    match widget_cache.borrow_mut().entry(*shape_id) {
                        Entry::Occupied(mut e) => {
                            e.get_mut()
                                .animate_into(draw_state, start, now, &mut xform_buffer.buffer)
                                .unwrap();
                        }
                        Entry::Vacant(e) => {
                            let mut widgets = part.widgets().read().clone();
                            widgets
                                .animate_into(draw_state, start, now, &mut xform_buffer.buffer)
                                .unwrap();
                            e.insert(widgets);
                        }
                    }
                });
            },
        );
    }

    pub fn track_state_changes(
        &mut self,
        _start: &Instant,
        _now: &Instant,
        _camera: &ScreenCamera,
        _world: &mut World,
    ) {
        /*
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
            let pos = transform.cartesian_km().point64().to_homogeneous();
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
         */
    }

    fn sys_ts_push_values_to_block(
        mut shapes: ResMut<ShapeBuffer>,
        query: Query<(
            &ShapeId,
            &SlotId,
            &ShapeTransformBuffer,
            &ShapeFlagBuffer,
            &ShapeXformBuffer,
        )>,
    ) {
        for (shape_id, slot_id, transform_buffer, flag_buffer, xform_buffer) in query.iter() {
            let xform_count = shapes.chunk_man.part(*shape_id).xform_count();
            shapes.push_values(
                *slot_id,
                &transform_buffer.buffer,
                flag_buffer.buffer,
                &Some(xform_buffer.buffer),
                xform_count,
            )
        }
    }

    fn sys_upload_block_frame_data(
        shapes: Res<ShapeBuffer>,
        gpu: Res<Gpu>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            for block in shapes.blocks.values() {
                block.make_upload_buffer(&gpu, encoder);
            }
        }
    }

    fn sys_draw_shapes(
        shapes: Res<ShapeBuffer>,
        globals: Res<GlobalParametersBuffer>,
        atmosphere: Res<AtmosphereBuffer>,
        world: Res<WorldRenderPass>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            let (color_attachments, depth_stencil_attachment) = world.offscreen_target_preserved();
            let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                label: Some("shape-draw"),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
            };
            let mut rpass = encoder.begin_render_pass(&render_pass_desc_ref);

            assert_ne!(LocalGroup::ShapeChunk.index(), Group::Globals.index());
            assert_ne!(LocalGroup::ShapeChunk.index(), Group::Atmosphere.index());
            assert_ne!(LocalGroup::ShapeBlock.index(), Group::Globals.index());
            assert_ne!(LocalGroup::ShapeBlock.index(), Group::Atmosphere.index());
            rpass.set_pipeline(&shapes.pipeline);
            rpass.set_bind_group(Group::Globals.index(), globals.bind_group(), &[]);
            rpass.set_bind_group(Group::Atmosphere.index(), atmosphere.bind_group(), &[]);

            for block in shapes.blocks.values() {
                let chunk = shapes.chunk_man.chunk(block.chunk_id());

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
                        // 0..1,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use animate::TimeStep;
    use camera::{CameraSystem, ScreenCameraController};
    use fullscreen::FullscreenBuffer;
    use lib::Libs;
    use orrery::Orrery;
    use stars::StarsBuffer;
    use terrain::TerrainBuffer;

    #[cfg(unix)]
    #[test]
    fn test_find_damage() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?
            .with_extension::<GlobalParametersBuffer>()?
            .with_extension::<AtmosphereBuffer>()?
            .with_extension::<ShapeBuffer>()?;
        let libs = Libs::for_testing()?;
        for (game, palette, catalog) in libs.selected() {
            if game.test_dir != "FA" {
                continue;
            }
            for fid in catalog.find_with_extension("SH")? {
                let meta = catalog.stat(fid)?;
                if meta.name() != "F22.SH" {
                    continue;
                }

                let results = runtime.resource_scope(|heap, mut inst_man: Mut<ShapeBuffer>| {
                    inst_man.upload_shapes(palette, &[meta.name()], catalog, heap.resource::<Gpu>())
                })?;
                println!("RESULT: {:#?}", results);
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_creation() -> Result<()> {
        let mut runtime = Gpu::for_test_unix()?;
        runtime
            .load_extension::<TimeStep>()?
            .load_extension::<Catalog>()?
            .load_extension::<CameraSystem>()?
            .load_extension::<GlobalParametersBuffer>()?
            .load_extension::<FullscreenBuffer>()?
            .load_extension::<AtmosphereBuffer>()?
            .load_extension::<ShapeBuffer>()?
            .load_extension::<StarsBuffer>()?
            .load_extension::<TerrainBuffer>()?
            .load_extension::<WorldRenderPass>()?
            .load_extension::<Orrery>()?;

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

        let libs = Libs::for_testing()?;

        for (game, palette, catalog) in libs.selected() {
            let mut all_shapes = Vec::new();
            for fid in catalog.find_with_extension("SH")? {
                let meta = catalog.stat(fid)?;
                if meta.name().ends_with("_S.SH")
                    || meta.name().ends_with("_A.SH")
                    || meta.name().ends_with("_B.SH")
                    || meta.name().ends_with("_C.SH")
                    || meta.name().ends_with("_D.SH")
                {
                    continue;
                }
                // FIXME: re-try all of these
                if skipped.contains(&meta.name()) {
                    println!(
                        "SKIP: {}:{:13} @ {}",
                        game.test_dir,
                        meta.name(),
                        meta.path()
                    );
                    continue;
                } else {
                    println!("At: {}:{:13} @ {}", game.test_dir, meta.name(), meta.path());
                }
                // FIXME: skip _?.SH shapes as those should be loaded automagically now
                all_shapes.push(meta.name().to_owned());
            }
            let out = runtime.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
                shapes.upload_shapes(palette, &all_shapes, catalog, heap.resource::<Gpu>())
            })?;

            // Create an instance of each core shape.
            for (name, shape_ids) in out.iter() {
                let id = runtime
                    .spawn_named(&format!("{}:{}", game.test_dir, name))?
                    .id();
                runtime.resource_scope(|mut heap, mut shapes: Mut<ShapeBuffer>| {
                    heap.resource_scope(|mut heap, gpu: Mut<Gpu>| {
                        shapes.instantiate(heap.named_entity_mut(id), shape_ids.normal(), &gpu)
                    })
                })?;
            }
        }

        // Crank frame to upload all the shapes we just loaded.
        let _player_ent = runtime
            .spawn_named("player")?
            .insert(WorldSpaceFrame::default())
            .insert(ScreenCameraController::default())
            .id();
        runtime.run_sim_once();
        runtime.run_frame_once();

        Ok(())
    }
}
