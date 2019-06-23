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
use log::trace;
use nalgebra::{Matrix4, Vector3};
use stars::Stars;
use std::{collections::HashSet, f32::consts::PI, sync::Arc};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    framebuffer::Subpass,
    impl_vertex,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
};
use window::GraphicsWindow;

const TAU: f32 = PI * 2f32;
const PI_2: f32 = PI / 2f32;

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
            }"
    }
}

const RADIUS: f32 = 0.0015f32;

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
        src: "
            #version 450

            // Constants
            #define PI 3.1415926538
            #define PI_2 (PI / 2.0)
            #define TAU (PI * 2.0)
            #define RADIUS (0.001 * 5.0)

            // Utility
            void v_to_ra_d(vec3 v, out float ra, out float dec) {
                ra = atan(v.x, v.z) + PI;
                dec = atan(v.y, sqrt(v.x * v.x + v.z * v.z));
            }

            vec3 ra_d_to_v(float ra, float dec) {
                return vec3(
                    cos(dec) * sin(ra),
                    -sin(dec),
                    cos(dec) * cos(ra)
                );
            }

            // Bin arrangement
            #define DEC_BINS 64
            struct BandMetadata {
                uint index;
                uint bins_per_row;
                uint base_index;
            };
            #define SHOW_BINS 0
            void show_bins();

            // Bin Info
            struct BinPosition {
                // Base offset into the star index buffer.
                uint index_base;

                // Number of stars in this bin.
                uint num_indexes;
            };

            struct StarInst {
                float ra;
                float dec;
                float color[3];
                float radius;
            };

            layout(location = 0) in vec3 v_ray;

            layout(location = 0) out vec4 f_color;

            layout(binding = 0) buffer DeclinationBands {
                BandMetadata arr[33];
            } bands;

            layout(binding = 1) buffer BinPositions {
                BinPosition arr[5434];
            } bins;

            layout(binding = 2) buffer Indexes {
                uint arr[];
            } indexes;

            layout(binding = 3) buffer StarBlock {
                StarInst arr[];
            } stars;

            BandMetadata band_for_dec(float dec) {
                // Project dec into 0..1
                float decz = ((dec + PI_2) * 2.0) / TAU;

                // Quantize dec into bands.
                uint deci = uint(decz * DEC_BINS);
                return bands.arr[deci];
            }

            uint bin_for_ra_d(float ra, float dec) {
                BandMetadata band = band_for_dec(dec);
                float raz = ra / TAU;
                uint rai = uint(float(band.bins_per_row) * raz);
                return band.base_index + rai;
            }

            void main() {
                #if SHOW_BINS
                    show_bins();
                    return;
                #endif

                float ra, dec;
                v_to_ra_d(v_ray, ra, dec);

                uint bin = bin_for_ra_d(ra, dec);
                BinPosition pos = bins.arr[bin];

                vec3 clr = vec3(0.0, 0.0, 0.0);
                for (uint i = pos.index_base; i < pos.index_base + pos.num_indexes; ++i) {
                    uint star_index = indexes.arr[i];
                    StarInst star = stars.arr[star_index];
                    vec3 star_ray = ra_d_to_v(star.ra, star.dec);
                    float dist = acos(dot(star_ray, normalize(v_ray)));
                    if (dist < star.radius) {
                        clr = vec3(
                            star.color[0], star.color[1], star.color[2]
                        );
                    }
                }

                f_color = vec4(clr, 1.0);
            }

            void show_bins() {
                float ra, dec;
                v_to_ra_d(v_ray, ra, dec);

                float raz = ra / TAU;
                float decz = ((dec + PI_2) * 2.0) / TAU;

                BandMetadata meta = band_for_dec(dec);
                uint rai = uint(raz * meta.bins_per_row);
                int deci = int(decz * DEC_BINS);

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
    pds: Arc<dyn DescriptorSet + Send + Sync>,
}

macro_rules! mkband {
    ($index:expr, $bins_per_row:expr, $base_index:expr) => {
        fs::ty::BandMetadata {
            index: $index,
            bins_per_row: $bins_per_row,
            base_index: $base_index,
        }
    };
}

const DEC_BINS: usize = 64;
const DEC_BANDS: [fs::ty::BandMetadata; 64] = [
    mkband!(0, 1, 0),
    mkband!(1, 8, 1),
    mkband!(2, 12, 9),
    mkband!(3, 16, 21),
    mkband!(4, 32, 37),
    mkband!(5, 32, 69),
    mkband!(6, 32, 101),
    mkband!(7, 32, 133),
    mkband!(8, 64, 165),
    mkband!(9, 64, 229),
    mkband!(10, 64, 293),
    mkband!(11, 64, 357),
    mkband!(12, 84, 421),
    mkband!(13, 84, 505),
    mkband!(14, 84, 589),
    mkband!(15, 84, 673),
    mkband!(16, 84, 757),
    mkband!(17, 84, 841),
    mkband!(18, 128, 925),
    mkband!(19, 128, 1053),
    mkband!(20, 128, 1181),
    mkband!(21, 128, 1309),
    mkband!(22, 128, 1437),
    mkband!(23, 128, 1565),
    mkband!(24, 128, 1693),
    mkband!(25, 128, 1821),
    mkband!(26, 128, 1949),
    mkband!(27, 128, 2077),
    mkband!(28, 128, 2205),
    mkband!(29, 128, 2333),
    mkband!(30, 128, 2461),
    mkband!(31, 128, 2589),
    mkband!(32, 128, 2717),
    mkband!(33, 128, 2845),
    mkband!(34, 128, 2973),
    mkband!(35, 128, 3101),
    mkband!(36, 128, 3229),
    mkband!(37, 128, 3357),
    mkband!(38, 128, 3485),
    mkband!(39, 128, 3613),
    mkband!(40, 128, 3741),
    mkband!(41, 128, 3869),
    mkband!(42, 128, 3997),
    mkband!(43, 128, 4125),
    mkband!(44, 128, 4253),
    mkband!(45, 128, 4381),
    mkband!(46, 84, 4509),
    mkband!(47, 84, 4593),
    mkband!(48, 84, 4677),
    mkband!(49, 84, 4761),
    mkband!(50, 84, 4845),
    mkband!(51, 84, 4929),
    mkband!(52, 64, 5013),
    mkband!(53, 64, 5077),
    mkband!(54, 64, 5141),
    mkband!(55, 64, 5205),
    mkband!(56, 32, 5269),
    mkband!(57, 32, 5301),
    mkband!(58, 32, 5333),
    mkband!(59, 32, 5365),
    mkband!(60, 16, 5397),
    mkband!(61, 12, 5413),
    mkband!(62, 8, 5425),
    mkband!(63, 1, 5433),
];

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

        let (vertex_buffer, index_buffer) = Self::build_buffers(window)?;

        let pds = Self::upload_stars(pipeline.clone(), window)?;

        Ok(Self {
            pipeline,
            push_constants: vs::ty::PushConstantData::new(),
            vertex_buffer,
            index_buffer,
            pds,
        })
    }

    fn get_perpendicular(v: &Vector3<f32>) -> Vector3<f32> {
        let not_v = if v[2] > v[1] {
            Vector3::new(0f32, 1f32, 0f32)
        } else {
            Vector3::new(0f32, 0f32, 1f32)
        };
        v.cross(&not_v)
    }

    fn ra_d_to_vec(ra: f32, dec: f32) -> Vector3<f32> {
        Vector3::new(dec.cos() * ra.sin(), -dec.sin(), dec.cos() * ra.cos())
    }

    fn vec_to_ra_d(v: &Vector3<f32>) -> (f32, f32) {
        let ra = v.x.atan2(v.z) + PI;
        let w = (v.x * v.x + v.z * v.z).sqrt();
        let dec = v.y.atan2(w);
        (ra, dec)
    }

    fn num_bins() -> usize {
        let last_band = &DEC_BANDS[DEC_BANDS.len() - 1];
        (last_band.base_index + last_band.bins_per_row) as usize
    }

    fn band_for_dec(dec: f32) -> &'static fs::ty::BandMetadata {
        assert!(dec < PI_2);
        assert!(dec >= -PI_2);
        // See implementation in shader for comments.
        let decz = ((dec + PI_2) * 2f32) / TAU;
        let deci = (decz * DEC_BINS as f32) as usize;
        &DEC_BANDS[deci]
    }

    fn bin_for_ra_d(ra: f32, dec: f32) -> usize {
        let band = Self::band_for_dec(dec);
        let raz = ra / TAU;
        let rai = (band.bins_per_row as f32 * raz) as usize;
        band.base_index as usize + rai
    }

    pub fn upload_stars(
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Arc<dyn DescriptorSet + Send + Sync>> {
        let mut offset = 0;
        for (i, band) in DEC_BANDS.iter().enumerate() {
            assert_eq!(band.index as usize, i);
            assert_eq!(band.base_index, offset);
            offset += band.bins_per_row;
        }

        let mut star_buf = Vec::new();
        let stars = Stars::new()?;
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            let ra = entry.right_ascension() as f32;
            let dec = entry.declination() as f32;
            let color = entry.color();
            let radius = RADIUS * entry.radius_scale();
            let star = fs::ty::StarInst {
                ra,
                dec,
                color,
                radius,
            };
            star_buf.push(star);
        }

        // Bin all stars.
        let mut bins = Vec::with_capacity(Self::num_bins());
        bins.resize_with(Self::num_bins(), HashSet::new);
        for (star_off, star) in star_buf.iter().enumerate() {
            // Sample bins in a circle at the star's approximate radius.
            let star_ray = Self::ra_d_to_vec(star.ra, star.dec);
            let perp = Self::get_perpendicular(&star_ray) * RADIUS * 2f32;
            let norm_star_ray = nalgebra::Unit::new_normalize(star_ray);
            const N_TAPS: usize = 4;
            for i in 0..N_TAPS {
                let ang = i as f32 * TAU / N_TAPS as f32;
                let rot = nalgebra::UnitQuaternion::from_axis_angle(&norm_star_ray, ang);
                let sample = (star_ray + (rot * perp)).normalize();
                let (sample_ra, sample_dec) = Self::vec_to_ra_d(&sample);
                bins[Self::bin_for_ra_d(sample_ra, sample_dec)].insert(star_off as u32);
            }
            bins[Self::bin_for_ra_d(star.ra, star.dec)].insert(star_off as u32);
        }

        // Now that we have sorted all stars into all bins they might affect,
        // build the index and bin position buffers.
        let mut bin_positions = Vec::new();
        let mut indices = Vec::new();
        for bin_indices in &bins {
            let bin_base = indices.len();
            let bin_len = bin_indices.len();

            let pos = fs::ty::BinPosition {
                index_base: bin_base as u32,
                num_indexes: bin_len as u32,
            };
            bin_positions.push(pos);
            for index in bin_indices {
                indices.push(*index as u32);
            }
        }

        println!(
            "uploading declination bands buffer with {} bytes",
            std::mem::size_of::<fs::ty::BandMetadata>() * DEC_BANDS.len()
        );
        let band_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            DEC_BANDS.iter().cloned(),
        )?;

        println!(
            "uploading bin position buffer with {} bytes",
            std::mem::size_of::<fs::ty::BinPosition>() * bin_positions.len()
        );
        let bin_pos_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            bin_positions.into_iter(),
        )?;

        println!(
            "uploading star index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let star_index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        println!(
            "uploading star buffer with {} bytes",
            std::mem::size_of::<fs::ty::StarInst>() * star_buf.len()
        );
        let star_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            star_buf.into_iter(),
        )?;

        let pds: Arc<dyn DescriptorSet + Send + Sync> = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_buffer(band_buffer.clone())?
                .add_buffer(bin_pos_buffer.clone())?
                .add_buffer(star_index_buffer.clone())?
                .add_buffer(star_buffer.clone())?
                .build()?,
        );

        Ok(pds)
    }

    pub fn build_buffers(
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

        Ok((vertex_buffer, index_buffer))
    }

    pub fn before_frame(&mut self, camera: &CameraAbstract) -> Fallible<()> {
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
    use super::StarboxRenderer as SB;
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn ra_d_to_bin_transform() -> Fallible<()> {
        for i in 0..32 {
            assert_eq!(SB::bin_for_ra_d(i as f32 / 32f32, -PI_2), 0);
            assert_eq!(SB::bin_for_ra_d(i as f32 / 32f32, PI_2 - 0.0001), 5433);
        }
        for i in 0..DEC_BINS {
            let f = i as f32 / DEC_BINS as f32;
            let dec = -PI_2 + f * PI + 0.001;
            let band = SB::band_for_dec(dec);
            assert_eq!(band.index, i as u32);

            for j in 0..band.bins_per_row {
                let g = j as f32 / band.bins_per_row as f32;
                let ra = g * TAU + 0.001;
                let bin_idx = SB::bin_for_ra_d(ra, dec);
                assert_eq!(bin_idx as u32, band.base_index + j);
            }
        }
        Ok(())
    }

    #[test]
    fn vec_to_rad_to_vec() -> Fallible<()> {
        let stars = Stars::new()?;
        for i in 0..stars.catalog_size() {
            let entry = stars.entry(i)?;
            let band = SB::band_for_dec(entry.declination());
            if entry.declination() < 0f32 {
                assert!(band.index < 32);
            } else {
                assert!(band.index >= 32);
            }
            let v = SB::ra_d_to_vec(entry.right_ascension(), entry.declination());
            let (ra, dec) = SB::vec_to_ra_d(&v);
            assert_relative_eq!(ra, entry.right_ascension(), epsilon = 0.00001);
            assert_relative_eq!(dec, entry.declination(), epsilon = 0.00001);
        }

        Ok(())
    }

    fn get_perpendicular(v: &Vector3<f32>) -> Vector3<f32> {
        let not_v = if v[2] > v[1] {
            Vector3::new(0f32, 1f32, 0f32)
        } else {
            Vector3::new(0f32, 0f32, 1f32)
        };
        v.cross(&not_v)
    }

    #[test]
    fn fast_taps() -> Fallible<()> {
        let v = Vector3::new(0f32, 0f32, 1f32);
        let perp = get_perpendicular(&v) * 0.001;
        let vn = nalgebra::Unit::new_normalize(v);

        for i in 0..8 {
            let ang = i as f32 * TAU / 8f32;
            let rot = nalgebra::UnitQuaternion::from_axis_angle(&vn, ang);
            let sample = v + rot * perp;
            println!("PERP: {}", sample);
        }

        Ok(())
    }
}
