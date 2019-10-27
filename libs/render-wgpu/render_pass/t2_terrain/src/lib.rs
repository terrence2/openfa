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
use atmosphere::AtmosphereBuffer;
use camera::CameraAbstract;
use failure::Fallible;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use log::trace;
use nalgebra::Vector3;
use t2_buffer::{T2Buffer, T2Vertex};
use wgpu;

pub struct FrameState {
    camera_upload_buffer: wgpu::Buffer,
    atmosphere_upload_buffer: wgpu::Buffer,
}

pub struct T2TerrainRenderPass {
    globals_buffer: GlobalParametersBuffer,
    atmosphere_buffer: AtmosphereBuffer,
    t2_buffer: T2Buffer,

    pipeline: wgpu::RenderPipeline,
}

impl T2TerrainRenderPass {
    pub fn new(gpu: &mut GPU, t2_buffer: T2Buffer) -> Fallible<Self> {
        trace!("T2TerrainRenderPass::new");

        let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
        let atmosphere_buffer = AtmosphereBuffer::new(gpu)?;

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/t2_terrain.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/t2_terrain.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        t2_buffer.bind_group_layout(),
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
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::Back,
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
                depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                    format: GPU::DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
                    stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
                    stencil_read_mask: 0,
                    stencil_write_mask: 0,
                }),
                index_format: wgpu::IndexFormat::Uint32,
                vertex_buffers: &[T2Vertex::descriptor()],
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self {
            globals_buffer,
            atmosphere_buffer,
            t2_buffer,
            pipeline,
        })
    }

    pub fn prepare_upload(
        &self,
        camera: &dyn CameraAbstract,
        sun_direction: &Vector3<f32>,
        device: &wgpu::Device,
    ) -> FrameState {
        FrameState {
            camera_upload_buffer: self.globals_buffer.make_upload_buffer(camera, device),
            atmosphere_upload_buffer: self.atmosphere_buffer.make_upload_buffer(
                camera,
                *sun_direction,
                device,
            ),
        }
    }

    pub fn upload(&self, frame: &mut gpu::Frame, state: FrameState) {
        self.globals_buffer
            .upload_from(frame, &state.camera_upload_buffer);
        self.atmosphere_buffer
            .upload_from(frame, &state.atmosphere_upload_buffer);
    }

    pub fn draw(&self, rpass: &mut wgpu::RenderPass) {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, self.globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(1, &self.atmosphere_buffer.bind_group(), &[]);
        rpass.set_bind_group(2, &self.t2_buffer.bind_group(), &[]);
        rpass.set_index_buffer(self.t2_buffer.index_buffer(), 0);
        rpass.set_vertex_buffers(0, &[(self.t2_buffer.vertex_buffer(), 0)]);
        rpass.draw_indexed(self.t2_buffer.index_range(), 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
