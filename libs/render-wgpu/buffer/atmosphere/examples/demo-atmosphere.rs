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
use atmosphere_wgpu::AtmosphereBuffer;
use camera::ArcBallCamera;
use failure::Fallible;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use log::trace;
use nalgebra::{convert, Matrix4, Point3, Unit, UnitQuaternion, Vector3};
use raymarching::{RaymarchingBuffer, RaymarchingVertex};
use std::{f64::consts::PI, time::Instant};
use wgpu;

fn main() -> Fallible<()> {
    let mut input = InputSystem::new(vec![InputBindings::new("base")
        .bind("+enter-move-sun", "mouse1")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let raymarching_buffer = RaymarchingBuffer::new(gpu.device())?;
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;

    let vert_shader = gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
    let frag_shader = gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[
                raymarching_buffer.bind_group_layout(),
                atmosphere_buffer.bind_group_layout(),
            ],
        });
    let pipeline = gpu
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vert_shader,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &frag_shader,
                entry_point: "main",
            }),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::None,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
            color_states: &[wgpu::ColorStateDescriptor {
                format: GPU::texture_format(),
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[RaymarchingVertex::descriptor()],
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.1, 3.4e+38);
    camera.set_target(6_378.1, 0.0, 0.0);
    camera.set_angle(PI / 2.0, -PI / 2.0);
    camera.set_up(-Vector3::x());
    camera.set_rotation(UnitQuaternion::from_axis_angle(
        &Unit::new_normalize(Vector3::z()),
        PI / 2.0,
    ));
    camera.set_distance(40.0);
    camera.on_mousebutton_down(1);
    let mut sun_angle = 0f64;
    let mut in_sun_move = false;

    let poll_start = Instant::now();
    gpu.device().poll(true);
    println!("poll time: {:?}", poll_start.elapsed());
    loop {
        let frame_start = Instant::now();
        for command in input.poll()? {
            match command.name.as_str() {
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "+enter-move-sun" => in_sun_move = true,
                "-enter-move-sun" => in_sun_move = false,
                "mouse-move" => {
                    if in_sun_move {
                        sun_angle += command.displacement()?.0 / (180.0 * 2.0);
                    } else {
                        camera.on_mousemove(
                            command.displacement()?.0 / 4.0,
                            command.displacement()?.1 / 4.0,
                        )
                    }
                }
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        // Prepare new camera parameters.
        let raymarching_upload_buffer =
            raymarching_buffer.make_upload_buffer(&camera, gpu.device());
        let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);
        let atmosphere_upload_buffer =
            atmosphere_buffer.make_upload_buffer(&camera, sun_direction, gpu.device());

        {
            let mut frame = gpu.begin_frame();
            {
                raymarching_buffer.upload_from(&mut frame, &raymarching_upload_buffer);
                atmosphere_buffer.upload_from(&mut frame, &atmosphere_upload_buffer);

                let mut rpass = frame.begin_render_pass();
                rpass.set_pipeline(&pipeline);
                rpass.set_bind_group(0, raymarching_buffer.bind_group(), &[]);
                rpass.set_bind_group(1, &atmosphere_buffer.bind_group(), &[]);
                rpass.set_vertex_buffers(0, &[(raymarching_buffer.vertex_buffer(), 0)]);
                rpass.draw(0..4, 0..1);
            }
            frame.finish();
        }

        println!("frame time: {:?}", frame_start.elapsed());
    }
    Ok(())
}

/*
use atmosphere::AtmosphereBuffers;
use camera::{ArcBallCamera, CameraAbstract};
use failure::Fallible;
use input::{InputBindings, InputSystem};
use log::trace;
use nalgebra::{convert, Matrix4, Point3, Unit, UnitQuaternion, Vector3};
use raymarching::{RaymarchingBuffer, RaymarchingVertex};
use std::{f64::consts::PI, sync::Arc};
use vulkano::{
    command_buffer::AutoCommandBufferBuilder,
    framebuffer::Subpass,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
};
use window::{GraphicsConfigBuilder, GraphicsWindow};

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
    include: ["./libs/render-vulkano"],
    src: "
        #version 450

        #include <buffer/raymarching/src/include_raymarching.glsl>

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
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
    include: ["./libs/render-vulkano"],
    src: "
        #version 450
        #include <common/include/include_global.glsl>

        layout(location = 0) in vec3 v_ray;
        layout(location = 1) in vec3 camera;
        layout(location = 2) in vec3 sun_direction;
        layout(location = 0) out vec4 f_color;

        #include <buffer/atmosphere/src/include_atmosphere.glsl>

        const float EXPOSURE = MAX_LUMINOUS_EFFICACY * 0.0001;

        #include <buffer/atmosphere/src/descriptorset_atmosphere.glsl>

        #include <buffer/atmosphere/src/draw_atmosphere.glsl>

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

            vec3 radiance = sky_radiance;
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

fn main() -> Fallible<()> {
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    window.hide_cursor()?;
    let bindings = InputBindings::new("base")
        .bind("+enter-move-sun", "mouse1")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![bindings]);

    let vert_shader = vs::Shader::load(window.device())?;
    let frag_shader = fs::Shader::load(window.device())?;
    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input_single_buffer::<RaymarchingVertex>()
            .vertex_shader(vert_shader.main_entry_point(), ())
            .triangle_strip()
            .cull_mode_back()
            .front_face_counter_clockwise()
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(frag_shader.main_entry_point(), ())
            .render_pass(
                Subpass::from(window.render_pass(), 0).expect("gfx: did not find a render pass"),
            )
            .build(window.device())?,
    ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>;
    let raymarching_renderer = RaymarchingBuffer::new(&window)?;
    let atmosphere = AtmosphereBuffers::new(pipeline.clone(), &window)?;
    let mut push_constants = vs::ty::PushConstantData::new();

    let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
    camera.set_target(6_378.1, 0.0, 0.0);
    camera.set_angle(PI / 2.0, -PI / 2.0);
    camera.set_up(-Vector3::x());
    camera.set_rotation(UnitQuaternion::from_axis_angle(
        &Unit::new_normalize(Vector3::z()),
        PI / 2.0,
    ));
    camera.set_distance(40.0);
    camera.on_mousebutton_down(1);
    let mut sun_angle = 0f64;
    let mut in_sun_move = false;

    let empty0 = GraphicsWindow::empty_descriptor_set(pipeline.clone(), 0)?;

    loop {
        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio_f64()?);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "+enter-move-sun" => in_sun_move = true,
                "-enter-move-sun" => in_sun_move = false,
                "mouse-move" => {
                    if in_sun_move {
                        sun_angle += command.displacement()?.0 / (180.0 * 2.0);
                    } else {
                        camera.on_mousemove(
                            command.displacement()?.0 / 4.0,
                            command.displacement()?.1 / 4.0,
                        )
                    }
                }
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }
        window.center_cursor()?;

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            push_constants.set_inverse_projection(camera.inverted_projection_matrix());
            push_constants.set_inverse_view(camera.inverted_view_matrix());
            push_constants.set_eye_position(convert(camera.get_target()));
            let sun_direction = Vector3::new(sun_angle.sin() as f32, 0f32, sun_angle.cos() as f32);
            push_constants.set_sun_direction(&sun_direction);

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = cbb.draw_indexed(
                pipeline.clone(),
                &window.dynamic_state,
                vec![raymarching_renderer.vertex_buffer.clone()],
                raymarching_renderer.index_buffer.clone(),
                (empty0.clone(), atmosphere.descriptor_set()),
                push_constants,
            )?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }
    }
}
*/
