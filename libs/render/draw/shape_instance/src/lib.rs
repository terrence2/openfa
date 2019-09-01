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
use lib::Library;
use nalgebra::Matrix4;
use nalgebra::Point3;
use omnilib::OmniLib;
use pal::Palette;
use shape_chunk::{Chunk, ClosedChunk, DrawSelection, OpenChunk, ShapeId};
use std::{collections::HashMap, mem};
use vulkano::{
    buffer::{CpuAccessibleBuffer, CpuBufferPool},
    command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DrawIndirectCommand},
};
use window::GraphicsWindow;
use world::{Entity, World};

const BLOCK_SIZE: usize = 128;

// Fixed reservation blocks for upload of a number of entities. Unfortunately, because of
// xforms, we don't know exactly how many instances will fit in any given block.
pub struct DynamicInstanceBlock {
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

pub struct ShapeInstanceRenderer {
    //per_chunk: Vec<ChunkInstances>,
    open_mover_chunk: OpenChunk,
    mover_chunks: Vec<ClosedChunk>,
}

impl ShapeInstanceRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        Ok(Self {
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

    /*
    fn find_open_mover_chunk(&mut self, window: &GraphicsWindow) -> Fallible<&mut OpenChunk> {
        for instances in &mut self.per_chunk {
            if instances.chunk.is_open() {
                return Ok(instances.chunk.as_open_chunk_mut());
            }
        }

        unimplemented!()
        /*
        self.per_chunk.push(ChunkInstances {
            chunk: Chunk::Open(OpenChunk::new(window)?),
            blocks: Vec::new(),
        });
        self.find_open_mover_chunk(window)
        */
    }
    */
}

use specs::prelude::*;
use world::component::{ShapeMesh, Transform};

struct ShapeRenderSystem;

impl<'a> System<'a> for ShapeRenderSystem {
    // These are the resources required for execution.
    // You can also define a struct and `#[derive(SystemData)]`,
    // see the `full` example.
    type SystemData = (ReadStorage<'a, Transform>, ReadStorage<'a, ShapeMesh>);

    fn run(&mut self, (transform, shape_mesh): Self::SystemData) {
        // The `.join()` combines multiple components,
        // so we only access those entities which have
        // both of them.

        // This joins the component storages for Position
        // and Velocity together; it's also possible to do this
        // in parallel using rayon's `ParallelIterator`s.
        // See `ParJoin` for more.
        for (transform, shape_mesh) in (&transform, &shape_mesh).join() {
            println!("shape_id: {:?}", shape_mesh.shape_id());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use window::GraphicsConfigBuilder;
    use world::World;

    #[test]
    fn it_works() -> Fallible<()> {
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;

        let window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let lib = omni.library("FA");

        let mut world = World::new(lib)?;

        let mut shape_renderer = ShapeInstanceRenderer::new(&window)?;
        let t80_id =
            shape_renderer.upload_mover("T80.SH", DrawSelection::NormalModel, &world, &window)?;

        let t80_ent = world.create_ground_mover(t80_id, Point3::new(0f64, 0f64, 0f64))?;

        let mut dispatcher = DispatcherBuilder::new()
            .with(ShapeRenderSystem, "", &[])
            .build();
        //dispatcher.dispatch(&mut world);
        world.run(&mut dispatcher);

        Ok(())
    }
}

/*
pub struct Entity {
    id: u64,
}

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

pub struct ShapeInstanceRenderer {
    open_chunk: Option<OpenChunk>,
    chunks: Vec<ClosedChunk>,

    // Map from the shape name to the chunk that shape is uploaded in.
    chunk_map: HashMap<String, usize>,

    // Map from the entity to the chunk instance they belong in.
    instance_map: HashMap<Entity, usize>,
}

impl ShapeInstanceRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        Ok(Self {
            open_chunk: Some(OpenChunk::new(window)?),
            chunks: Vec::new(),
        })
    }

    pub fn add_static_immortal_model(
        &mut self,
        // TODO: position and orientation
        shape_name: &str,
        pal: &Palette,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        ensure!(
            self.open_chunk.is_some(),
            "shape instances are already finished"
        );

        // Note: immortal implies a non-damage model
        self.open_chunk.as_mut().unwrap().upload_shape(
            shape_name,
            DrawSelection::NormalModel,
            &pal,
            &lib,
            &window,
        )?;

        Ok(())
    }

    // Close the last open chunk and prepare for rendering.
    pub fn finish_loading() {}
}

*/
