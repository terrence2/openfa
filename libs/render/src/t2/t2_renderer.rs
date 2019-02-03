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
use crate::t2::texture_atlas::TextureAtlas;
use asset::AssetManager;
use failure::Fallible;
use image::{ImageBuffer, Rgba};
use lay::Layer;
use lib::Library;
use log::trace;
use mm::{MissionMap, TLoc};
use nalgebra::Matrix4;
use pal::Palette;
use pic::decode_pic;
use std::{collections::HashMap, sync::Arc};
use t2::Terrain;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    framebuffer::Subpass,
    image::{Dimensions, ImmutableImage},
    impl_vertex,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    tex_coord: [f32; 2],
}

impl_vertex!(Vertex, position, color, tex_coord);

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec3 position;
            layout(location = 1) in vec4 color;
            layout(location = 2) in vec2 tex_coord;

            layout(push_constant) uniform PushConstantData {
              mat4 projection;
            } pc;

            layout(location = 0) out vec4 v_color;
            layout(location = 1) out vec2 v_tex_coord;

            void main() {
                gl_Position = pc.projection * vec4(position, 1.0);
                v_color = color;
                v_tex_coord = tex_coord;
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
        src: "
            #version 450

            layout(location = 0) in vec4 v_color;
            layout(location = 1) in vec2 v_tex_coord;

            layout(location = 0) out vec4 f_color;

            layout(set = 0, binding = 0) uniform sampler2D tex;

            void main() {
                if (v_tex_coord.x == 0.0) {
                    f_color = v_color;
                } else {
                    f_color = texture(tex, v_tex_coord);
                }
            }
            "
    }
}

impl vs::ty::PushConstantData {
    fn new() -> Self {
        Self {
            projection: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
        }
    }

    fn set_projection(&mut self, mat: Matrix4<f32>) {
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

pub struct T2Renderer {
    mm: MissionMap,
    terrain: Arc<Box<Terrain>>,
    layer: Arc<Box<Layer>>,
    pic_data: HashMap<TLoc, Vec<u8>>,
    base_palette: Palette,
    pub used_palette: Palette,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    push_constants: vs::ty::PushConstantData,
    pds: Option<Arc<dyn DescriptorSet + Send + Sync>>,
    vertex_buffer: Option<Arc<CpuAccessibleBuffer<[Vertex]>>>,
    index_buffer: Option<Arc<CpuAccessibleBuffer<[u32]>>>,
}

impl T2Renderer {
    pub fn new(
        mm: MissionMap,
        assets: &Arc<Box<AssetManager>>,
        lib: &Arc<Box<Library>>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        trace!("T2Renderer::new");
        let terrain_name = mm.find_t2_for_map(&|s| lib.file_exists(s))?;
        let terrain = assets.load_t2(&terrain_name)?;

        // The following are used in FA:
        //    cloud1b.LAY 1
        //    day2b.LAY 0
        //    day2b.LAY 4
        //    day2e.LAY 0
        //    day2f.LAY 0
        //    day2.LAY 0
        //    day2v.LAY 0
        let layer = assets.load_lay(&mm.layer_name.to_uppercase())?;

        let mut pic_data = HashMap::new();
        let texture_base_name = mm.get_base_texture_name()?;
        for (_pos, tmap) in &mm.tmaps {
            if !pic_data.contains_key(&tmap.loc) {
                let name = tmap.loc.pic_file(&texture_base_name);
                let data = lib.load(&name)?.to_vec();
                pic_data.insert(tmap.loc.clone(), data);
            }
        }

        let base_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

        let vs = vs::Shader::load(window.device())?;
        let fs = fs::Shader::load(window.device())?;

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(vs.main_entry_point(), ())
                .triangle_strip()
                .cull_mode_back()
                .front_face_counter_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
                .depth_stencil_simple_depth()
                .blend_alpha_blending()
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        );

        let mut t2_renderer = Self {
            mm,
            terrain,
            layer,
            pic_data,
            base_palette: base_palette.clone(),
            used_palette: base_palette,
            pipeline,
            push_constants: vs::ty::PushConstantData::new(),
            pds: None,
            vertex_buffer: None,
            index_buffer: None,
        };
        let (pds, vertex_buffer, index_buffer, palette) =
            t2_renderer.regenerate_with_palette_parameters(window, 0, 0, 0, 0, 0)?;
        t2_renderer.vertex_buffer = Some(vertex_buffer);
        t2_renderer.index_buffer = Some(index_buffer);
        t2_renderer.pds = Some(pds);
        t2_renderer.used_palette = palette;

        Ok(t2_renderer)
    }

    pub fn set_projection(&mut self, projection: Matrix4<f32>) {
        self.push_constants.set_projection(projection);
    }

    pub fn render(
        &self,
        command_buffer: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        Ok(command_buffer.draw_indexed(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.vertex_buffer.clone().unwrap()],
            self.index_buffer.clone().unwrap(),
            self.pds.clone().unwrap(),
            self.push_constants,
        )?)
    }

    pub fn set_palette_parameters(
        &mut self,
        window: &GraphicsWindow,
        lay_base: i32,
        e0_off: i32,
        f1_off: i32,
        c2_off: i32,
        d3_off: i32,
    ) -> Fallible<()> {
        let (pds, vertex_buffer, index_buffer, palette) = self
            .regenerate_with_palette_parameters(window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
        self.pds = Some(pds);
        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
        self.used_palette = palette;
        Ok(())
    }

    fn regenerate_with_palette_parameters(
        &self,
        window: &GraphicsWindow,
        lay_base: i32,
        e0_off: i32,
        f1_off: i32,
        c2_off: i32,
        d3_off: i32,
    ) -> Fallible<(
        Arc<dyn DescriptorSet + Send + Sync>,
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
        Palette,
    )> {
        // Note: we need to really find the right palette.
        let mut palette = self.base_palette.clone();
        let layer_data = self.layer.for_index(self.mm.layer_index + 2, lay_base)?;
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
        // FIXME: this is close except on TVIET, which needs some fiddling around 0xC0.
        palette.overlay_at(&r2, (0xC0 + c2_off) as usize)?;
        palette.overlay_at(&r3, (0xD0 + d3_off) as usize)?;
        palette.overlay_at(&r0, (0xE0 + e0_off) as usize)?;
        palette.overlay_at(&r1, (0xF0 + f1_off) as usize)?;

        //palette.dump_png("terrain_palette")?;

        // Texture counts for all FA T2's.
        // APA: 68 x 256 (6815744 texels)
        // BAL: 66 x 256
        // CUB: 66 x 256
        // EGY: 49 x 256
        // FRA: 47 x 256
        // GRE: 68
        // IRA: 51 x 256
        // KURILE: 236 (Kxxxxxx) x 128/256 (33554432 texels)
        // LFA: 68
        // NSK: 68
        // PGU: 51
        // SPA: 49
        // TVIET: 42 (TVI) x 256
        // UKR: 29
        // VLA: 52
        // WTA: 68

        // Load all images with our new palette.
        let mut pics = Vec::new();
        for (tloc, data) in &self.pic_data {
            let pic = decode_pic(&palette, data)?;
            pics.push((tloc.clone(), pic));
        }

        let atlas = TextureAtlas::new(pics)?;

        let (texture, tex_future) = Self::upload_texture_rgba(window, atlas.img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;

        let (vertex_buffer, index_buffer) =
            self.upload_terrain_textured_simple(&atlas, &palette, window)?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        Ok((pds, vertex_buffer, index_buffer, palette))
    }

    fn sample_at(&self, palette: &Palette, xi: u32, zi: u32) -> ([f32; 3], [f32; 4]) {
        let offset = (zi * self.terrain.width + xi) as usize;
        let s = if offset < self.terrain.samples.len() {
            self.terrain.samples[offset]
        } else {
            let offset = ((zi - 1) * self.terrain.width + xi) as usize;
            if offset < self.terrain.samples.len() {
                self.terrain.samples[offset]
            } else {
                let offset = ((zi - 1) * self.terrain.width + (xi - 1)) as usize;
                self.terrain.samples[offset]
            }
        };

        let x = xi as f32 / (self.terrain.width as f32) - 0.5;
        let z = zi as f32 / (self.terrain.height as f32) - 0.5;
        let h = -(s.height as f32) / (256.0f32 * 2f32);

        let mut c = palette.rgba(s.color as usize).unwrap();
        if s.color == 0xFF {
            c.data[3] = 0;
        }

        (
            [x, h, z],
            [
                c[0] as f32 / 255f32,
                c[1] as f32 / 255f32,
                c[2] as f32 / 255f32,
                c[3] as f32 / 255f32,
            ],
        )
    }

    fn upload_terrain_textured_simple(
        &self,
        atlas: &TextureAtlas,
        palette: &Palette,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
    )> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        for zi_base in (0..self.terrain.height).step_by(4) {
            for xi_base in (0..self.terrain.width).step_by(4) {
                let base = verts.len() as u32;

                // Upload all vertices in patch.
                if let Some(tmap) = self.mm.tmaps.get(&(xi_base, zi_base)) {
                    let frame = &atlas.frames[&tmap.loc];

                    for z_off in 0..5 {
                        for x_off in 0..5 {
                            let zi = zi_base + z_off;
                            let xi = xi_base + x_off;
                            let (position, _samp_color) = self.sample_at(palette, xi, zi);

                            verts.push(Vertex {
                                position,
                                color: [0f32, 0f32, 0f32, 0f32],
                                tex_coord: frame.interp(
                                    x_off as f32 / 4f32,
                                    z_off as f32 / 4f32,
                                    &tmap.orientation,
                                )?,
                            });
                        }
                    }
                } else {
                    for z_off in 0..5 {
                        for x_off in 0..5 {
                            let zi = zi_base + z_off;
                            let xi = xi_base + x_off;
                            let (position, color) = self.sample_at(palette, xi, zi);

                            verts.push(Vertex {
                                position,
                                color,
                                tex_coord: [0f32, 0f32],
                            });
                        }
                    }
                }

                // There is a fixed strip pattern here that we could probably make use of.
                // For now just re-compute per patch with the base offset.
                for row in 0..4 {
                    let row_off = row * 5;

                    indices.push(base + row_off + 0);
                    indices.push(base + row_off + 0);

                    for column in 0..5 {
                        indices.push(base + row_off + column);
                        indices.push(base + row_off + column + 5);
                    }

                    indices.push(base + row_off + 4 + 5);
                    indices.push(base + row_off + 4 + 5);
                }
            }
        }

        trace!(
            "uploading vertex buffer with {} bytes",
            std::mem::size_of::<Vertex>() * verts.len()
        );
        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

        trace!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        Ok((vertex_buffer, index_buffer))
    }

    fn upload_texture_rgba(
        window: &GraphicsWindow,
        image_buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
    ) -> Fallible<(Arc<ImmutableImage<Format>>, Box<GpuFuture>)> {
        let image_dim = image_buf.dimensions();
        let image_data = image_buf.into_raw().clone();

        let dimensions = Dimensions::Dim2d {
            width: image_dim.0,
            height: image_dim.1,
        };
        let (texture, tex_future) = ImmutableImage::from_iter(
            image_data.iter().cloned(),
            dimensions,
            Format::R8G8B8A8Unorm,
            window.queue(),
        )?;
        Ok((texture, Box::new(tex_future) as Box<GpuFuture>))
    }

    fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
            Filter::Nearest,
            Filter::Nearest,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        )?;

        Ok(sampler)
    }
}
