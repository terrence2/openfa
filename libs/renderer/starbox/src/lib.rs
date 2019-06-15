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
use image::{ImageBuffer, Rgba};
use log::trace;
use nalgebra::{Matrix4, Vector3};
use stars::Stars;
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
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

#[derive(Copy, Clone)]
pub struct Vertex {
    position: [f32; 2],
}

impl_vertex!(Vertex, position);

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec2 position;

            layout(push_constant) uniform PushConstantData {
              mat4 inverse_projection;
              mat4 inverse_view;
            } pc;

            layout(location = 0) out vec3 v_ray;

            void main() {
                vec4 reverse_vec;

                // inverse perspective projection
                reverse_vec = vec4(position, 0.0, 1.0);
                reverse_vec = pc.inverse_projection * reverse_vec;

                // inverse modelview, without translation
                reverse_vec.w = 0.0;
                reverse_vec = pc.inverse_view * reverse_vec;

                v_ray = vec3(reverse_vec);
                gl_Position = vec4(position.xy, 0.0, 1.0);

                // gl_Position = pc.inverse_projection * vec4(position, 0.0, 1.0);
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
        src: "
            #version 450
            #define PI 3.1415926538

            layout(location = 0) in vec3 v_ray;

            layout(location = 0) out vec4 f_color;

            // layout(binding = 1) buffer UniformMatrixArray {
            //     mat4 xforms[14];
            // } ua;

            struct StarInst {
                float ra;
                float dec;
                uint color;
                uint pad;
            };

            layout(binding = 0) buffer StarBlock {
                StarInst arr[];
            } stars;

            layout(binding = 1) buffer StarBins {
                int ndarr[512][256][10];
                //int arr[];
            } bins;

            //layout(set = 0, binding = 0) uniform sampler2D tex;

            void main() {
                float ra = atan(v_ray.x, v_ray.z) + PI;
                float w = sqrt(v_ray.x * v_ray.x + v_ray.z * v_ray.z);
                float dec = atan(v_ray.y, w);

                float raz = ra / (2.0 * PI);
                float decz = (dec + PI) / (2.0 * PI);

                int rai = int(raz * 512.0);
                int deci = int(decz * 256.0);

                vec3 clr = vec3(0.0, 0.0, 0.0);

                /*
                int cnt = 0;
                int base_idx = rai * 256 * 10 + deci * 10;
                for (int i = 0; i < 10; ++i) {
                    if (bins.arr[base_idx + i] != 0xFFFFFFFF) {
                        StarInst star = stars.arr[base_idx + i];
                        float dd = dec - star.dec;
                        float dr = ra - star.ra;
                        float dist = sqrt(dd * dd + dr * dr);
                        // if (dist < 0.6) {
                        //     clr = vec3(1.0, 1.0, 1.0);
                        // }
                        clr = vec3(dist, dist, dist);
                        cnt += 1;
                    }
                }
                */
                for (int i = 0; i < 10; ++i) {
                    int idx = bins.ndarr[rai][deci][i];
                    if (idx != 0xFFFFFFFF) {
                        StarInst star = stars.arr[idx];
                        float dd = dec - star.dec;
                        float dr = ra - star.ra;
                        float dist = sqrt(dd * dd + dr * dr);
                        if (dist < 0.001) {
                            clr += vec3(
                                1.0 - dist * 1000.0,
                                1.0 - dist * 1000.0,
                                1.0 - dist * 1000.0
                            );
                        }
                        // if (dist < 0.001) {
                        //     clr = vec3(1.0, 1.0, 1.0);
                        // }
                        //clr = vec3(dist, dist, dist);
                    }
                }

                // f_color = vec4(
                //     float(cnt) / 10.0,
                //     float(cnt) / 10.0,
                //     float(cnt) / 10.0,
                //     1.0
                // );
                f_color = vec4(clr, 1.0);

                /*
                if ((rai & 1) != 0) {
                    if ((deci & 1) != 0) {
                        f_color = vec4(0.0, 1.0, 1.0, 1.0);
                    } else {
                        f_color = vec4(1.0, 0.0, 1.0, 1.0);
                    }
                } else {
                    if ((deci & 1) != 0) {
                        f_color = vec4(1.0, 0.0, 1.0, 1.0);
                    } else {
                        f_color = vec4(0.0, 1.0, 1.0, 1.0);
                    }
                }
                */
            }
            "
    }
}

impl vs::ty::PushConstantData {
    fn new() -> Self {
        Self {
            inverse_projection: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
            inverse_view: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
        }
    }

    fn set_inverse_projection(&mut self, mat: Matrix4<f32>) {
        self.inverse_projection[0][0] = mat[0];
        self.inverse_projection[0][1] = mat[1];
        self.inverse_projection[0][2] = mat[2];
        self.inverse_projection[0][3] = mat[3];
        self.inverse_projection[1][0] = mat[4];
        self.inverse_projection[1][1] = mat[5];
        self.inverse_projection[1][2] = mat[6];
        self.inverse_projection[1][3] = mat[7];
        self.inverse_projection[2][0] = mat[8];
        self.inverse_projection[2][1] = mat[9];
        self.inverse_projection[2][2] = mat[10];
        self.inverse_projection[2][3] = mat[11];
        self.inverse_projection[3][0] = mat[12];
        self.inverse_projection[3][1] = mat[13];
        self.inverse_projection[3][2] = mat[14];
        self.inverse_projection[3][3] = mat[15];
    }

    fn set_inverse_view(&mut self, mat: Matrix4<f32>) {
        self.inverse_view[0][0] = mat[0];
        self.inverse_view[0][1] = mat[1];
        self.inverse_view[0][2] = mat[2];
        self.inverse_view[0][3] = mat[3];
        self.inverse_view[1][0] = mat[4];
        self.inverse_view[1][1] = mat[5];
        self.inverse_view[1][2] = mat[6];
        self.inverse_view[1][3] = mat[7];
        self.inverse_view[2][0] = mat[8];
        self.inverse_view[2][1] = mat[9];
        self.inverse_view[2][2] = mat[10];
        self.inverse_view[2][3] = mat[11];
        self.inverse_view[3][0] = mat[12];
        self.inverse_view[3][1] = mat[13];
        self.inverse_view[3][2] = mat[14];
        self.inverse_view[3][3] = mat[15];
    }
}

pub struct StarboxRenderer {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    push_constants: vs::ty::PushConstantData,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
    star_buffer: Arc<CpuAccessibleBuffer<[fs::ty::StarInst]>>,
    pds: Arc<dyn DescriptorSet + Send + Sync>,
}

impl StarboxRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        trace!("StarboxRenderer::new");

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
                /*
                .depth_stencil(DepthStencil {
                    depth_write: false,
                    depth_compare: Compare::GreaterOrEqual,
                    depth_bounds_test: DepthBounds::Disabled,
                    stencil_front: Default::default(),
                    stencil_back: Default::default(),
                })
                */
                //.blend_alpha_blending()
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        );

        let (vertex_buffer, index_buffer) = Self::build_buffers(pipeline.clone(), window)?;

        let (star_buffer, pds) = Self::upload_stars(pipeline.clone(), window)?;

        Ok(Self {
            pipeline,
            push_constants: vs::ty::PushConstantData::new(),
            vertex_buffer,
            index_buffer,
            star_buffer,
            pds,
        })
    }

    pub fn upload_stars(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<CpuAccessibleBuffer<[fs::ty::StarInst]>>,
        Arc<dyn DescriptorSet + Send + Sync>,
    )> {
        const MAG: f32 = 6.5;

        let mut star_buf = Vec::new();
        let stars = Stars::new()?;
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            if entry.magnitude() <= MAG {
                let ra = entry.right_ascension() as f32;
                let dec = entry.declination() as f32;
                let color = entry.color();
                let star = fs::ty::StarInst {
                    ra,
                    dec,
                    color,
                    pad: 0,
                };
                star_buf.push(star);
            }
        }

        // Flat array of 512 x 256 x 10
        use std::f32::consts::PI;
        const RA_BINS: usize = 512;
        const DEC_BINS: usize = 256;
        const IDX_BINS: usize = 10;
        const CAPACITY: usize = RA_BINS * DEC_BINS * IDX_BINS;
        let mut bins = Vec::with_capacity(CAPACITY);
        const TOMBSTONE: u32 = 0xFFFF_FFFF;
        bins.resize(CAPACITY, TOMBSTONE);
        for (star_off, star) in star_buf.iter().enumerate() {
            let ra = star.ra;
            let dec = star.dec;
            let ra_bin = (ra * RA_BINS as f32 / (PI * 2f32)) as usize;
            let dec_bin = (((dec + PI) * DEC_BINS as f32) / (PI * 2f32)) as usize;
            let base_idx = ra_bin * DEC_BINS * IDX_BINS + dec_bin * IDX_BINS;
            for bin in bins[base_idx..base_idx + IDX_BINS].iter_mut() {
                if *bin == TOMBSTONE {
                    *bin = star_off as u32;
                }
            }
        }

        trace!(
            "uploading star buffer with {} bytes",
            std::mem::size_of::<fs::ty::StarInst>() * star_buf.len()
        );
        let star_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            star_buf.into_iter(),
        )?;

        trace!(
            "uploading star index buffer with {} bytes",
            std::mem::size_of::<u32>() * bins.len()
        );
        let bin_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), bins.into_iter())?;

        let pds: Arc<dyn DescriptorSet + Send + Sync> = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_buffer(star_buffer.clone())?
                .add_buffer(bin_buffer.clone())?
                //.add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        Ok((star_buffer, pds))
    }

    pub fn build_buffers(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
    )> {
        // Compute vertices such that we can handle any aspect ratio, or set up the camera to handle this?
        let x0 = -1f32;
        let x1 = 1f32;
        let y0 = -1f32;
        let y1 = 1f32;
        let verts = vec![
            Vertex { position: [x0, y0] },
            Vertex { position: [x0, y1] },
            Vertex { position: [x1, y0] },
            Vertex { position: [x1, y1] },
        ];
        let indices = vec![0u32, 1u32, 2u32, 3u32];

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

        /*
        let (texture, tex_future) = Self::upload_texture_rgba(window, img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;
        */

        Ok((vertex_buffer, index_buffer))
    }

    /*
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
        trace!(
            "uploading texture with {} bytes",
            image_dim.0 * image_dim.1 * 4
        );
        Ok((texture, Box::new(tex_future) as Box<GpuFuture>))
    }

    fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
            Filter::Linear,
            Filter::Linear,
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
    */

    /*
    pub fn set_projection(&mut self, window: &GraphicsWindow) -> Fallible<()> {
        let dim = window.dimensions()?;
        let aspect = window.aspect_ratio()? * 4f32 / 3f32;
        if dim[0] > dim[1] {
            self.push_constants
                .set_projection(Matrix4::new_nonuniform_scaling(&Vector3::new(
                    aspect, 1f32, 1f32,
                )));
        } else {
            self.push_constants
                .set_projection(Matrix4::new_nonuniform_scaling(&Vector3::new(
                    1f32,
                    1f32 / aspect,
                    1f32,
                )));
        }
        Ok(())
    }
    */

    pub fn before_frame(
        &mut self,
        camera: &CameraAbstract,
        window: &GraphicsWindow,
    ) -> Fallible<()> {
        //self.set_projection(&window)?;
        self.push_constants
            .set_inverse_projection(camera.inverted_projection_matrix());
        self.push_constants
            .set_inverse_view(camera.inverted_view_matrix());
        Ok(())
    }

    pub fn render(
        &self,
        cb: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let mut cb = cb;
        cb = cb.draw_indexed(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.vertex_buffer.clone()],
            self.index_buffer.clone(),
            self.pds.clone(),
            self.push_constants,
        )?;

        Ok(cb)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
