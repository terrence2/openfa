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
use camera::{ArcBallCamera, CameraAbstract};
use failure::Fallible;
use frame_graph::CopyBufferDescriptor;
use nalgebra::{convert, Isometry3, Matrix4, Point3, Unit, UnitQuaternion, Vector3};
use std::{cell::RefCell, f32::consts::PI, mem, sync::Arc};
use t2::Terrain;
use wgpu;

// FIXME: these should probably not live here.
const HM_TO_KM: f32 = 1f32 / 10f32;

pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<Box<wgpu::Buffer>>,
}

#[derive(Copy, Clone, Debug)]
struct Globals {
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    inv_proj: [[f32; 4]; 4],
    camera_position_tile: [f32; 4],
    camera_position_earth_km: [f32; 4],
}

impl GlobalParametersBuffer {
    pub fn new(device: &wgpu::Device) -> Fallible<Arc<RefCell<Self>>> {
        let buffer_size = mem::size_of::<Globals>() as wgpu::BufferAddress;
        let parameters_buffer = Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
            size: buffer_size,
            usage: wgpu::BufferUsage::STORAGE_READ | wgpu::BufferUsage::COPY_DST,
        })));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[wgpu::BindGroupLayoutBinding {
                binding: 0,
                visibility: wgpu::ShaderStage::all(),
                ty: wgpu::BindingType::StorageBuffer {
                    dynamic: false,
                    readonly: true,
                },
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &parameters_buffer,
                    range: 0..buffer_size,
                },
            }],
        });

        Ok(Arc::new(RefCell::new(Self {
            bind_group_layout,
            bind_group,
            buffer_size,
            parameters_buffer,
        })))
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn make_upload_buffer(
        &self,
        camera: &ArcBallCamera,
        device: &wgpu::Device,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        let globals = [Self::generic_camera_to_buffer(camera)];
        let source = device
            .create_buffer_mapped::<Globals>(
                1,
                wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
            )
            .fill_from_slice(&globals);
        upload_buffers.push(CopyBufferDescriptor::new(
            source,
            self.parameters_buffer.clone(),
            self.buffer_size,
        ));
        Ok(())
    }

    fn generic_camera_to_buffer(camera: &dyn CameraAbstract) -> Globals {
        fn m2v(m: &Matrix4<f32>) -> [[f32; 4]; 4] {
            let mut v = [[0f32; 4]; 4];
            for i in 0..16 {
                v[i / 4][i % 4] = m[i];
            }
            v
        }
        fn p2v(p: &Point3<f32>) -> [f32; 4] {
            [p.x, p.y, p.z, 0f32]
        }
        Globals {
            view: m2v(&camera.view_matrix()),
            proj: m2v(&camera.projection_matrix()),
            inv_view: m2v(&camera.inverted_view_matrix()),
            inv_proj: m2v(&camera.inverted_projection_matrix()),
            camera_position_tile: p2v(&camera.position()),
            camera_position_earth_km: p2v(&(camera.position() * HM_TO_KM)),
        }
    }

    pub fn make_upload_buffer_for_arcball_in_tile(
        &self,
        terrain: &Terrain,
        camera: &ArcBallCamera,
        device: &wgpu::Device,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        let globals = [Self::arcball_camera_to_buffer(terrain, camera)];
        let source = device
            .create_buffer_mapped::<Globals>(
                1,
                wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
            )
            .fill_from_slice(&globals);
        upload_buffers.push(CopyBufferDescriptor::new(
            source,
            self.parameters_buffer.clone(),
            self.buffer_size,
        ));
        Ok(())
    }

    fn arcball_camera_to_buffer(terrain: &Terrain, camera: &ArcBallCamera) -> Globals {
        fn m2v(m: &Matrix4<f32>) -> [[f32; 4]; 4] {
            let mut v = [[0f32; 4]; 4];
            for i in 0..16 {
                v[i / 4][i % 4] = m[i];
            }
            v
        }
        fn p2v(p: &Point3<f32>) -> [f32; 4] {
            [p.x, p.y, p.z, 0f32]
        }
        fn deg2rad(deg: f32) -> f32 {
            deg * PI / 180f32
        }
        fn ft2hm(ft: f32) -> f32 {
            ft * 0.003_048
        }

        let tile_width_ft = terrain.extent_east_west_in_ft();
        let tile_height_ft = terrain.extent_north_south_in_ft();
        let tile_width_hm = ft2hm(tile_width_ft);
        let tile_height_hm = ft2hm(tile_height_ft);

        let lat = deg2rad(terrain.origin_latitude());
        let lon = deg2rad(terrain.origin_longitude());

        /*
        fn rad2deg(rad: f32) -> f32 {
            rad * 180f32 / PI
        }
        let ft_per_degree = lat.cos() * 69.172f32 * 5_280f32;
        let angular_height = tile_height_ft as f32 / ft_per_degree;
        println!(
            "\"{}\": TL coord: {}, {}",
            terrain.name(),
            rad2deg(lat + deg2rad(angular_height)),
            rad2deg(lon)
        );
        */

        // Lat/Lon to XYZ in KM.
        // x = (N + h) * cos(lat) * cos(lon)
        // y = (N + h) * cos(lat) * sin(lon)
        // z = (( b^2 / a^2 ) * N + h) * sin(lat)
        let base = Point3::new(lat.cos() * lon.sin(), -lat.sin(), lat.cos() * lon.cos());
        let base_in_km = base * 6360f32;

        let r_lon = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0f32, -1f32, 0f32)),
            -lon,
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(r_lon * Vector3::new(1f32, 0f32, 0f32)),
            -(PI / 2f32 - lat),
        );

        let tile_ul_eye = camera.position();
        let tile_ul_tgt: Point3<f32> = convert(camera.target);
        let ul_to_c = Vector3::new(tile_width_hm / 2f32, 0f32, tile_height_hm / 2f32);
        let tile_c_eye = tile_ul_eye - ul_to_c;
        let tile_c_tgt = tile_ul_tgt - ul_to_c;
        let tile_up: Vector3<f32> = convert(camera.up);

        let earth_eye = r_lat * r_lon * (tile_c_eye * HM_TO_KM) + base_in_km.coords;
        let earth_tgt = r_lat * r_lon * (tile_c_tgt * HM_TO_KM) + base_in_km.coords;
        let earth_up = r_lat * r_lon * tile_up;

        let earth_view = Isometry3::look_at_rh(&earth_eye, &earth_tgt, &earth_up);

        let earth_inv_view: Matrix4<f32> = earth_view.inverse().to_homogeneous();
        let earth_inv_proj: Matrix4<f32> = convert(camera.projection().inverse());

        Globals {
            view: m2v(&camera.view_matrix()),
            proj: m2v(&camera.projection_matrix()),
            inv_view: m2v(&earth_inv_view),
            inv_proj: m2v(&earth_inv_proj),
            camera_position_tile: p2v(&camera.position()),
            camera_position_earth_km: p2v(&earth_eye),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu::GPU;
    use input::InputSystem;

    #[test]
    fn it_can_create_a_buffer() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let gpu = GPU::new(&input, Default::default())?;
        let _globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
        Ok(())
    }
}
