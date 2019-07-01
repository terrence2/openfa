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
              vec4 eye_position;
            } pc;

            layout(location = 0) out vec3 v_ray;
            layout(location = 1) out flat vec3 fl_eye;

            void main() {
                vec4 reverse_vec;

                // inverse perspective projection
                reverse_vec = vec4(position, 0.0, 1.0);
                reverse_vec = pc.inverse_projection * reverse_vec;

                // inverse modelview, without translation
                reverse_vec.w = 0.0;
                reverse_vec = pc.inverse_view * reverse_vec;

                v_ray = vec3(reverse_vec);
                fl_eye = pc.eye_position.xyz;
                gl_Position = vec4(position.xy, 0.0, 1.0);
            }"
    }
}

const RADIUS: f32 = 6_378f32; // km

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
            #define RADIUS 6378.0

            layout(location = 0) in vec3 v_ray;
            layout(location = 1) in vec3 fl_eye;

            layout(location = 0) out vec4 f_color;

            float density(float h) {
                float a0 =  7.001985e-2;
                float a1 = -4.336216e-3;
                float a2 = -5.009831e-3;
                float a3 =  1.621827e-4;
                float a4 = -2.471283e-6;
                float a5 =  1.904383e-8;
                float a6 = -7.189421e-11;
                float a7 =  1.060067e-13;
                float exponent = ((((((a7*h + a6)*h + a5)*h + a4)*h + a3)*h + a2)*h + a1)*h + a0;
                return pow(10, exponent);
            }

            // Assumption: sphere is centered
            bool ray_sphere_intersect(
                vec3 ray_origin,
                vec3 ray_direction,
                float sphere_radius,
                out vec3 intersect,
                out bool inside
            )
            {
                vec3 sphere_to_ray = ray_origin;
                float a = dot(ray_direction, ray_direction);
                float b = 2.0 * dot(sphere_to_ray, ray_direction);
                float r2 = sphere_radius * sphere_radius;
                float c = dot(sphere_to_ray, sphere_to_ray) - r2;
                float discriminant = b * b - 4 * a * c;
                if (discriminant < 0) {
                    return false;
                }
                float t0 = (-b - sqrt(discriminant)) / (2.0 * a);
                float t1 = (-b + sqrt(discriminant)) / (2.0 * a);
                if (t0 < 0 && t1 < 0) {
                    return false;
                }
                if (t0 < 0 && t1 > 0) {
                    inside = true;
                    intersect = ray_origin + t1 * ray_direction;
                } else {
                    inside = false;
                    intersect = ray_origin + t0 * ray_direction;
                }
                return true;
            }

            void main() {
                vec3 ray = normalize(v_ray);
                vec3 intersect;
                bool under_water;
                bool collide = ray_sphere_intersect(
                    fl_eye,
                    ray,
                    RADIUS,
                    intersect,
                    under_water
                );
                if (collide) {
                    vec3 water = normalize(intersect);
                    if (under_water) {
                        float dist = length(intersect - fl_eye);
                        f_color = vec4(water / (dist * 10), 1.0);
                    } else {
                        f_color = vec4(water, 1.0);
                    }
                } else {
                    // Spaaaaace!
                    f_color = vec4(0.0, 0.0, 0.0, 1.0);
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
            eye_position: [0f32, 0f32, 0f32, 0f32],
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

    fn set_eye_position(&mut self, v: Vector3<f32>) {
        self.eye_position[0] = v[0];
        self.eye_position[1] = v[1];
        self.eye_position[2] = v[2];
        self.eye_position[3] = 1f32;
    }
}

pub struct SkyRenderer {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    push_constants: vs::ty::PushConstantData,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
    //pds: Arc<dyn DescriptorSet + Send + Sync>,
}

impl SkyRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        trace!("SkyRenderer::new");

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

        //let pds = Self::upload_stars(pipeline.clone(), window)?;

        Ok(Self {
            pipeline,
            push_constants: vs::ty::PushConstantData::new(),
            vertex_buffer,
            index_buffer,
            //pds,
        })
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
        self.push_constants
            .set_eye_position(camera.position() / 1000f32);
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
            (), //self.pds.clone(),
            self.push_constants,
        )?;

        Ok(cb)
    }
}

#[cfg(test)]
mod tests {
    use super::SkyRenderer as SR;
    use super::*;
    use approx::assert_relative_eq;
}
