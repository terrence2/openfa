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
use gpu::GPU;
use std::{collections::HashMap, sync::Arc};
use wgpu;

pub trait GraphBuffer {
    fn name(&self) -> &'static str;
    fn bind_group_layout(&self) -> &wgpu::BindGroupLayout;
    fn bind_group(&self) -> &wgpu::BindGroup;
}

pub trait Constructable {
    fn new(gpu: &GPU, layouts: &[&wgpu::BindGroupLayout]) -> Arc<Box<dyn FrameRenderPass>>;
}

pub trait FrameRenderPass {
    fn pipeline(&self) -> &wgpu::RenderPipeline;
}

pub struct RenderPassManager {
    pass: Arc<Box<dyn FrameRenderPass>>,
    inputs: Vec<Arc<Box<dyn GraphBuffer>>>,
    // outputs: Vec<Arc<Box<dyn GraphBuffer>>>,
}

pub struct FrameGraph {
    buffers: HashMap<String, Arc<Box<dyn GraphBuffer>>>,
    passes: Vec<RenderPassManager>,
}

pub struct FrameGraphBuilder<'a> {
    gpu: &'a GPU,
    buffers: HashMap<String, Arc<Box<dyn GraphBuffer>>>,
    passes: Vec<RenderPassManager>,
}

impl<'a> FrameGraphBuilder<'a> {
    pub fn new(gpu: &'a GPU) -> Self {
        Self {
            gpu,
            buffers: HashMap::new(),
            passes: Vec::new(),
        }
    }

    /*
    pub fn with_buffer<T: ?Sized>(mut self, buffer: Arc<Box<T>>) -> Self
    where
        Arc<Box<dyn GraphBuffer>>: From<Arc<Box<T>>>,
    {
        let gb = <Arc<Box<dyn GraphBuffer>>>::from(buffer);
        //self.buffers.insert(buffer.name().to_owned(), buffer.clone());
        self
    }
    */

    pub fn with_buffer(mut self, buffer: Arc<Box<dyn GraphBuffer>>) -> Self {
        //self.buffers.insert(buffer.name().to_owned(), buffer.clone());
        self
    }

    pub fn with_pass<T: Constructable>(mut self, inputs: &[&str], _outputs: &[&str]) -> Self {
        let input_buffers = inputs
            .iter()
            .map(|&s| self.buffers[s].clone())
            .collect::<Vec<_>>();
        let constructor_layouts = input_buffers
            .iter()
            .map(|b| b.bind_group_layout())
            .collect::<Vec<_>>();
        let pass = T::new(self.gpu, &constructor_layouts);
        self.passes.push(RenderPassManager {
            pass: pass.clone(),
            inputs: input_buffers,
        });
        self
    }

    pub fn build(self) -> FrameGraph {
        FrameGraph {
            buffers: self.buffers,
            passes: self.passes,
        }
    }
}

pub struct UploadState {}

impl FrameGraph {
    pub fn prepare_upload(&self) -> UploadState {
        UploadState {}
    }

    pub fn render(&self) {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
