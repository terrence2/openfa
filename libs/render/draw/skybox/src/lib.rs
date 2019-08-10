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

// Accumulate all depthless raymarching passes into one draw operation.

use atmosphere::AtmosphereBuffers;
use base::{RayMarchingRenderer, RayMarchingVertex};
use camera::CameraAbstract;
use failure::Fallible;
use log::trace;
use nalgebra::{Matrix4, Point3, Vector3};
use stars::StarsBuffers;
use std::sync::Arc;
use vulkano::{
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::DescriptorSet,
    framebuffer::Subpass,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
};
use window::GraphicsWindow;

mod vs {
    vulkano_shaders::shader! {
    ty: "vertex",
    include: ["./libs/renderer/base/src"],
    src: "
        #version 450

        #include \"include_raymarching.glsl\"

        layout(push_constant) uniform PushConstantData {
            mat4 inverse_view;
            mat4 inverse_projection;
            vec4 eye_position;
            vec4 sun_direction;
        } pc;

        layout(location = 0) in vec2 position;
        layout(location = 0) out vec3 v_ray;
        layout(location = 1) out flat vec3 camera;
        layout(location = 2) out flat vec3 sun_direction;

        void main() {
            v_ray = raymarching_view_ray(position, pc.inverse_view, pc.inverse_projection);
            camera = pc.eye_position.xyz;
            sun_direction = pc.sun_direction.xyz;
            gl_Position = vec4(position, 0.0, 1.0);
        }"
    }
}

mod fs {
    vulkano_shaders::shader! {
    ty: "fragment",
    include: ["./libs/render"],
    src: "
        #version 450
        #include <common/include/include_global.glsl>

        layout(location = 0) in vec3 v_ray;
        layout(location = 1) in vec3 camera;
        layout(location = 2) in vec3 sun_direction;
        layout(location = 0) out vec4 f_color;

        #include <buffer/atmosphere/src/include_atmosphere.glsl>
        #include <buffer/stars/src/include_stars.glsl>

        const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;

        #include <buffer/atmosphere/src/descriptorset_atmosphere.glsl>
        #include <buffer/stars/src/descriptorset_stars.glsl>

        #include <buffer/atmosphere/src/draw_atmosphere.glsl>
        #include <buffer/stars/src/draw_stars.glsl>

        void main() {
            vec3 view = normalize(v_ray);

            vec3 ground_radiance;
            float ground_alpha;
            compute_ground_radiance(
                cd.atmosphere,
                transmittance_texture,
                scattering_texture,
                single_mie_scattering_texture,
                irradiance_texture,
                camera,
                view,
                sun_direction,
                ground_radiance,
                ground_alpha);

            vec3 sky_radiance = vec3(0);
            compute_sky_radiance(
                cd.atmosphere,
                transmittance_texture,
                scattering_texture,
                single_mie_scattering_texture,
                irradiance_texture,
                camera,
                view,
                sun_direction,
                sky_radiance
            );

            vec3 star_radiance;
            float star_alpha = 0.5;
            show_stars(view, star_radiance, star_alpha);

            vec3 radiance = sky_radiance + star_radiance * star_alpha;
            radiance = mix(radiance, ground_radiance, ground_alpha);

            vec3 color = pow(
                vec3(1.0) - exp(-radiance / vec3(cd.atmosphere.whitepoint) * EXPOSURE),
                vec3(1.0 / 2.2)
            );
            f_color = vec4(color, 1.0);
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
            sun_direction: [1f32, 0f32, 0f32, 0f32],
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

    fn set_eye_position(&mut self, p: Point3<f32>) {
        self.eye_position[0] = p[0];
        self.eye_position[1] = p[1];
        self.eye_position[2] = p[2];
        self.eye_position[3] = 1f32;
    }

    fn set_sun_direction(&mut self, v: &Vector3<f32>) {
        self.sun_direction[0] = v[0];
        self.sun_direction[1] = v[1];
        self.sun_direction[2] = v[2];
        self.sun_direction[3] = 0f32;
    }
}

pub struct SkyboxRenderer {
    raymarching_renderer: RayMarchingRenderer,
    empty_ds0: Arc<dyn DescriptorSet + Send + Sync>,
    atmosphere_buffers: AtmosphereBuffers,
    stars_buffers: StarsBuffers,

    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    push_constants: vs::ty::PushConstantData,
}

impl SkyboxRenderer {
    pub fn new(window: &GraphicsWindow) -> Fallible<Self> {
        trace!("SkyboxRenderer::new");

        let vert_shader = vs::Shader::load(window.device())?;
        let frag_shader = fs::Shader::load(window.device())?;
        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<RayMarchingVertex>()
                .vertex_shader(vert_shader.main_entry_point(), ())
                .triangle_strip()
                .cull_mode_back()
                .front_face_counter_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(frag_shader.main_entry_point(), ())
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>;
        let push_constants = vs::ty::PushConstantData::new();
        let raymarching_renderer = RayMarchingRenderer::new(pipeline.clone(), &window)?;
        let atmosphere_buffers =
            AtmosphereBuffers::new(&raymarching_renderer, pipeline.clone(), &window)?;
        let stars_buffers = StarsBuffers::new(&raymarching_renderer, pipeline.clone(), &window)?;
        let empty_ds0 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 0)?;

        Ok(Self {
            raymarching_renderer,
            empty_ds0,
            atmosphere_buffers,
            stars_buffers,
            pipeline,
            push_constants,
        })
    }

    pub fn before_frame(
        &mut self,
        camera: &CameraAbstract,
        sun_direction: &Vector3<f32>,
    ) -> Fallible<()> {
        self.push_constants
            .set_inverse_projection(camera.inverted_projection_matrix());
        self.push_constants
            .set_inverse_view(camera.inverted_view_matrix());

        // Camera is in meters, but we draw the (massive) skybox in kilometers because f32.
        self.push_constants
            .set_eye_position(camera.position() / 1000.0);

        // FIXME: this should take an orrery that can give us the direction exactly.
        self.push_constants.set_sun_direction(&sun_direction);

        Ok(())
    }

    pub fn draw(
        &self,
        cbb: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        Ok(cbb.draw_indexed(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.raymarching_renderer.vertex_buffer.clone()],
            self.raymarching_renderer.index_buffer.clone(),
            (
                self.empty_ds0.clone(),
                self.atmosphere_buffers.descriptor_set(),
                self.stars_buffers.descriptor_set(),
            ),
            self.push_constants,
        )?)
    }
}
