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
use crate::sh::texture_atlas::TextureAtlas;
use failure::{ensure, Fallible};
use image::{ImageBuffer, Rgba};
use lib::Library;
use log::trace;
use nalgebra::Matrix4;
use pal::Palette;
use pic::decode_pic;
use sh::{CpuShape, FacetFlags, Instr};
use std::sync::Arc;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    framebuffer::Subpass,
    image::{Dimensions, ImmutableImage},
    impl_vertex,
    pipeline::{
        depth_stencil::{Compare, DepthBounds, DepthStencil},
        GraphicsPipeline, GraphicsPipelineAbstract,
    },
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
              mat4 view;
              mat4 projection;
            } pc;

            layout(location = 0) out vec4 v_color;
            layout(location = 1) out vec2 v_tex_coord;

            void main() {
                gl_Position = pc.projection * pc.view * vec4(position, 1.0);
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
                    vec4 tex_color = texture(tex, v_tex_coord);
                    if (tex_color.a < 0.5)
                        discard;
                    else
                        f_color = tex_color;
                }
            }
            "
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

    fn set_view(&mut self, mat: Matrix4<f32>) {
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

#[derive(Clone)]
pub struct ShInstance {
    push_constants: vs::ty::PushConstantData,
    pds: Arc<dyn DescriptorSet + Send + Sync>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
}

pub struct ShRenderer {
    system_palette: Arc<Palette>,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    instance: Option<ShInstance>,
}

impl ShRenderer {
    pub fn new(system_palette: Arc<Palette>, window: &GraphicsWindow) -> Fallible<Self> {
        trace!("ShRenderer::new");

        let vs = vs::Shader::load(window.device())?;
        let fs = fs::Shader::load(window.device())?;

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(vs.main_entry_point(), ())
                .triangle_list()
                .cull_mode_back()
                .front_face_counter_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
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
        );
        Ok(ShRenderer {
            system_palette,
            pipeline,
            instance: None,
        })
    }

    pub fn set_projection(&mut self, projection: &Matrix4<f32>) {
        self.instance
            .as_mut()
            .unwrap()
            .push_constants
            .set_projection(projection);
    }

    pub fn set_view(&mut self, view: Matrix4<f32>) {
        self.instance
            .as_mut()
            .unwrap()
            .push_constants
            .set_view(view);
    }

    pub fn add_shape_to_render(
        &mut self,
        _name: &str,
        sh: &CpuShape,
        lib: &Library,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        let mut _xform = [0f32, 0f32, 0f32, 0f32, 0f32, 0f32];

        let mut textures = Vec::new();
        for filename in sh.all_textures() {
            let img = decode_pic(&self.system_palette, &lib.load(&filename.to_uppercase())?)?;
            textures.push((filename, img));
        }
        let atlas = TextureAtlas::new(textures)?;
        let mut active_frame = None;

        // The current pool of vertices.
        let mut vert_pool = Vec::new();

        // We pull from the vert buffer as needed to build faces, because the color and
        // texture information is specified per face.
        let mut indices = Vec::new();
        let mut verts = Vec::new();

        let mut _byte_offset = 0;
        let mut offset = 0;
        while offset < sh.instrs.len() {
            let instr = &sh.instrs[offset];
            println!("At: {} => {}", offset, instr.show());

            match instr {
                Instr::Header(_hdr) => {
                    _xform = [0f32, 0f32, 0f32, 0f32, 0f32, 0f32];
                }
                Instr::TextureRef(texture) => {
                    active_frame = Some(&atlas.frames[&texture.filename]);
                }
                Instr::F2_JumpIfNotShown(f2) => {
                    //                    if f2.next_offset() > self.end_at_offset {
                    //                        self.end_at_offset = f2.next_offset();
                    //                    }
                    trace!("JUMP IF NOT SHOWN: {}", f2.next_offset());
                }
                Instr::UnkC8_JumpOnDetailLevel(c8) => {
                    //                    if c8.next_offset() < self.subdetail_at_offset {
                    //                        self.subdetail_at_offset = c8.next_offset();
                    //                    }
                    trace!("JUMP ON DETAIL LEVEL: {}", c8.next_offset());
                }
                Instr::UnkC4(c4) => {
                    // C4 00   FF FF   13 00   E4 FF    00 00   00 00   00 00    7D 02
                    //            -1      19     -28        0       0     ang      637
                    #[allow(clippy::cast_ptr_alignment)] // the entire point of word codes
                    let vp = unsafe { std::slice::from_raw_parts(c4.data as *const u16, 7) };
                    //let vp: &[i16] = unsafe { mem::transmute(&c4.data.offset(2)) };
                    println!(
                        "v: ({}, {}, {}), ang: ({}, {}, {}), ?: {}",
                        vp[0], vp[1], vp[2], vp[3], vp[4], vp[5], vp[6],
                    );
                    _xform = [
                        f32::from(vp[0]),
                        f32::from(vp[1]),
                        f32::from(vp[2]),
                        f32::from(vp[3]),
                        f32::from(vp[4]),
                        f32::from(vp[5]),
                    ];
                }
                Instr::VertexBuf(buf) => {
                    if !vert_pool.is_empty() {
                        break;
                    }
                    // if buf.unk0 & 1 == 1 {
                    //     vert_pool.truncate(0);
                    // }
                    // if end_at_offset == buf.offset {
                    //     vert_pool.truncate(0);
                    // }
                    for v in &buf.verts {
                        vert_pool.push(Vertex {
                            position: [f32::from(v[0]), f32::from(-v[2]), f32::from(v[1])],
                            color: [0.75f32, 0.5f32, 0f32, 1f32],
                            tex_coord: [0f32, 0f32],
                        });
                    }
                }
                Instr::Facet(facet) => {
                    // Load all vertices in this facet into the vertex upload buffer, copying
                    // in the color and texture coords for each face.
                    let mut v_base = verts.len() as u32;
                    for i in 2..facet.indices.len() {
                        // Given that most facets are very short strips, and we need to copy the
                        // vertices anyway, it is more space efficient to just upload triangle
                        // lists instead of trying to span safely between adjacent strips.
                        let o = if i % 2 == 0 {
                            [i - 2, i - 1, i]
                        } else {
                            [i - 3, i - 1, i]
                        };
                        let inds = [
                            facet.indices[o[0]],
                            facet.indices[o[1]],
                            facet.indices[o[2]],
                        ];
                        let tcs = [
                            facet.tex_coords[o[0]],
                            facet.tex_coords[o[1]],
                            facet.tex_coords[o[2]],
                        ];

                        for (index, tex_coord) in inds.iter().zip(&tcs) {
                            ensure!(
                                (*index as usize) < vert_pool.len(),
                                "out-of-bounds vertex reference in facet {:?}, current pool size: {}",
                                facet,
                                vert_pool.len()
                            );
                            let mut v = vert_pool[*index as usize];
                            if facet.flags.contains(FacetFlags::HAVE_TEXCOORDS) {
                                assert!(active_frame.is_some());
                                let frame = active_frame.unwrap();
                                v.tex_coord = frame.tex_coord_at(*tex_coord);
                            }
                            verts.push(v);
                            indices.push(v_base);
                            v_base += 1;
                        }
                    }
                }
                _ => {}
            }
            offset += 1;
            _byte_offset += instr.size();
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

        let (texture, tex_future) = Self::upload_texture_rgba(window, atlas.img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        let inst = ShInstance {
            push_constants: vs::ty::PushConstantData::new(),
            pds,
            vertex_buffer,
            index_buffer,
        };

        self.instance = Some(inst);

        Ok(())
    }

    pub fn render(
        &self,
        command_buffer: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let inst = self.instance.clone().unwrap();
        Ok(command_buffer.draw_indexed(
            self.pipeline.clone(),
            dynamic_state,
            vec![inst.vertex_buffer.clone()],
            inst.index_buffer.clone(),
            inst.pds.clone(),
            inst.push_constants,
        )?)
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

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;
    use sh::CpuShape;
    use window::GraphicsConfigBuilder;

    #[test]
    fn it_can_render_shapes() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        let omnilib = OmniLib::new_for_test()?;
        let lib = omnilib.library("FA");
        let system_palette = Arc::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?);
        let sh = CpuShape::from_bytes(&lib.load("BNK1.SH")?)?;
        let mut sh_renderer = ShRenderer::new(system_palette, &window)?;
        sh_renderer.add_shape_to_render("foo", &sh, &lib, &window)?;
        window.drive_frame(|command_buffer, dynamic_state| {
            sh_renderer.render(command_buffer, dynamic_state)
        })?;
        std::mem::drop(window);
        Ok(())
    }
}
