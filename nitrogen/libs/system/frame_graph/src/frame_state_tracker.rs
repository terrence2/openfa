// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use std::sync::Arc;

pub struct CopyBufferDescriptor {
    pub source: wgpu::Buffer,
    pub source_offset: wgpu::BufferAddress,
    pub destination: Arc<Box<wgpu::Buffer>>,
    pub destination_offset: wgpu::BufferAddress,
    pub copy_size: wgpu::BufferAddress,
}

impl CopyBufferDescriptor {
    pub fn new(
        source: wgpu::Buffer,
        destination: Arc<Box<wgpu::Buffer>>,
        copy_size: wgpu::BufferAddress,
    ) -> Self {
        Self {
            source,
            source_offset: 0,
            destination,
            destination_offset: 0,
            copy_size,
        }
    }
}

// Note: still quite limited; just precompute without dependencies.
pub struct FrameStateTracker {
    uploads: Vec<CopyBufferDescriptor>,
    precompute: Vec<i32>, // TODO
}

impl Default for FrameStateTracker {
    fn default() -> Self {
        Self {
            uploads: Vec::new(),
            precompute: Vec::new(),
        }
    }
}

impl FrameStateTracker {
    pub fn reset(&mut self) {
        self.uploads.clear();
        self.precompute.clear();
    }

    pub fn upload(
        &mut self,
        source: wgpu::Buffer,
        destination: Arc<Box<wgpu::Buffer>>,
        copy_size: usize,
    ) {
        assert!(copy_size < wgpu::BufferAddress::MAX as usize);
        self.upload_ba(source, destination, copy_size as wgpu::BufferAddress);
    }

    pub fn upload_ba(
        &mut self,
        source: wgpu::Buffer,
        destination: Arc<Box<wgpu::Buffer>>,
        copy_size: wgpu::BufferAddress,
    ) {
        self.uploads
            .push(CopyBufferDescriptor::new(source, destination, copy_size));
    }

    pub fn drain_uploads(&mut self) -> impl Iterator<Item = CopyBufferDescriptor> + '_ {
        self.uploads.drain(..)
    }
}
