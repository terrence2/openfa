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
use backend::backend::{Backend, Device as BackendDevice};
use failure::{err_msg, Fallible};
use hal::{
    buffer, format::Format, image, mapping, memory, pool::CommandPoolCreateFlags, Adapter,
    Backend as HalBackend, CommandPool, Device, Graphics, Limits, MemoryType, MemoryTypeId,
    PhysicalDevice, QueueGroup, Surface,
};

pub struct UploadBuffer<'a> {
    gpu: &'a Gpu,
    buffer: <Backend as HalBackend>::Buffer,
    memory: <Backend as HalBackend>::Memory,
    requirements: memory::Requirements,
}

impl<'a> UploadBuffer<'a> {
    pub fn new(
        gpu: &'a Gpu,
        buffer: <Backend as HalBackend>::Buffer,
        memory: <Backend as HalBackend>::Memory,
        requirements: memory::Requirements,
    ) -> Self {
        Self {
            gpu,
            buffer,
            memory,
            requirements,
        }
    }
}

impl<'a> Drop for UploadBuffer<'a> {
    fn drop(&mut self) {
        // FIXME: do we need to sync with other GPU resources here?
        self.gpu.device.destroy_buffer(self.buffer);
        //self.gpu.device.free_memory(self.memory);
    }
}

pub struct ImageBuffer<'a> {
    gpu: &'a Gpu,
    //buffer: <Backend as HalBackend>::Buffer,
    buffer: HalBackend::Buffer,
    memory: <Backend as HalBackend>::Memory,
    requirements: memory::Requirements,
}

pub struct Gpu {
    command_pool: CommandPool<Backend, Graphics>,
    queue_group: QueueGroup<Backend, Graphics>,
    device: BackendDevice,
    limits: Limits,
    memory_types: Vec<MemoryType>,
}

impl Gpu {
    pub fn new(adapter: &mut Adapter<Backend>, surface: &Box<Surface<Backend>>) -> Fallible<Self> {
        let (device, queue_group) =
            adapter.open_with::<_, Graphics>(1, |family| surface.supports_queue_family(family))?;

        let mut command_pool =
            device.create_command_pool_typed(&queue_group, CommandPoolCreateFlags::empty(), 16);

        let limits = adapter.physical_device.limits();

        let memory_types = adapter.physical_device.memory_properties().memory_types;
        return Ok(Self {
            command_pool,
            queue_group,
            device,
            limits,
            memory_types,
        });
    }

    pub fn limits(&self) -> &Limits {
        return &self.limits;
    }

    pub fn create_upload_buffer(&self, upload_size: u64) -> Fallible<UploadBuffer> {
        let buffer_unbound = self
            .device
            .create_buffer(upload_size, buffer::Usage::TRANSFER_SRC)?;
        let requirements = self.device.get_buffer_requirements(&buffer_unbound);
        let upload_memory = self.device.allocate_memory(
            self.get_upload_memory_type(&requirements)?,
            requirements.size,
        )?;
        let upload_buffer = self
            .device
            .bind_buffer_memory(&upload_memory, 0, buffer_unbound)?;
        return Ok(UploadBuffer::new(
            self,
            upload_buffer,
            upload_memory,
            requirements,
        ));
    }

    pub fn create_image(&self, kind: image::Kind) -> Fallible<()> {
        let unbound = self.device.create_image(
            kind,
            1,
            Format::Rgba8Srgb,
            image::Tiling::Optimal,
            image::Usage::TRANSFER_DST | image::Usage::SAMPLED,
            image::ViewCapabilities::empty(),
        )?;
        let req = self.device.get_image_requirements(&unbound);
        let mem_type = self.get_image_memory_type(&req)?;
        // FIXME: free_memory on this
        let image_memory = self.device.allocate_memory(mem_type, req.size)?;
        // FIXME: destroy_image on this
        let image = self.device.bind_image_memory(&image_memory, 0, unbound)?;

        Ok(())
    }

    // FIXME: maybe don't panic on failure in the callback?
    pub fn with_mapped_upload_buffer<F>(&self, buffer: &UploadBuffer, f: F) -> Fallible<()>
    where
        F: FnOnce(&mut mapping::Writer<Backend, u8>),
    {
        let mut data = self
            .device
            .acquire_mapping_writer::<u8>(&buffer.memory, 0..buffer.requirements.size)?;
        f(&mut data);
        self.device.release_mapping_writer(data);
        return Ok(());
    }

    pub fn get_upload_memory_type(&self, require: &memory::Requirements) -> Fallible<MemoryTypeId> {
        return Ok(self
            .get_memory_type(require.type_mask, memory::Properties::CPU_VISIBLE)
            .ok_or_else(|| {
                err_msg(format!(
                    "gfx: no memory upload type for gpu: req:{:?}",
                    require
                ))
            })?.into());
    }

    pub fn get_image_memory_type(&self, require: &memory::Requirements) -> Fallible<MemoryTypeId> {
        return Ok(self
            .get_memory_type(require.type_mask, memory::Properties::DEVICE_LOCAL)
            .ok_or_else(|| {
                err_msg(format!(
                    "gfx: no image memory type for gpu: req:{:?}",
                    require
                ))
            })?.into());
    }

    pub fn get_memory_type(&self, type_mask: u64, property: memory::Properties) -> Option<usize> {
        return self
            .memory_types
            .iter()
            .enumerate()
            .position(|(id, mem_type)| {
                type_mask & (1 << id) != 0 && mem_type.properties.contains(property)
            });
    }
}

// impl Drop for Gpu {
//     fn drop(&mut self) {
//         self.device
//             .destroy_command_pool(self.command_pool.into_raw());
//     }
// }

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_can_create_gpu() -> Fallible<()> {
        let mut win = gfx::Window::new(800, 600, "test-texture")?;
        win.select_any_adapter()?;
        return Ok(());
    }
}
