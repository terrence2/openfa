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
use failure::Error;
use hal;
use hal::Device;
use hal::{pool::CommandPoolCreateFlags, Adapter, CommandPool, Graphics, QueueGroup, Surface};

pub struct Gpu {
    command_pool: CommandPool<Backend, Graphics>,
    queue_group: QueueGroup<Backend, Graphics>,
    device: BackendDevice,
}

impl Gpu {
    pub fn new(
        adapter: &mut Adapter<Backend>,
        surface: &Box<Surface<Backend>>,
    ) -> Result<Self, Error> {
        let (device, queue_group) =
            adapter.open_with::<_, Graphics>(1, |family| surface.supports_queue_family(family))?;

        let mut command_pool =
            device.create_command_pool_typed(&queue_group, CommandPoolCreateFlags::empty(), 16);

        return Ok(Self {
            command_pool,
            queue_group,
            device,
        });
    }
}

// impl Drop for Gpu {
//     fn drop(&mut self) {
//         self.device
//             .destroy_command_pool(self.command_pool.into_raw());
//     }
// }
