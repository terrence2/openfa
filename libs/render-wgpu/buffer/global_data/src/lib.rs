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
use camera::{ArcBallCamera, UfoCamera};
use failure::Fallible;
use frame_graph::CopyBufferDescriptor;
use gpu::GPU;
use nalgebra::{convert, Isometry3, Matrix4, Point3, Unit, UnitQuaternion, Vector3, Vector4};
use std::{cell::RefCell, f64::consts::PI, mem, sync::Arc};
use t2::Terrain;
use universe::{FEET_TO_HM_32, FEET_TO_HM_64};
use wgpu;
use zerocopy::{AsBytes, FromBytes};

// FIXME: these should probably not live here.
const HM_TO_KM: f64 = 1.0 / 10.0;

pub fn m2v(m: &Matrix4<f32>) -> [[f32; 4]; 4] {
    let mut v = [[0f32; 4]; 4];
    for i in 0..16 {
        v[i / 4][i % 4] = m[i];
    }
    v
}

pub fn p2v(p: &Point3<f32>) -> [f32; 4] {
    [p.x, p.y, p.z, 0f32]
}

pub fn v2v(v: &Vector4<f32>) -> [f32; 4] {
    [v[0], v[1], v[2], v[3]]
}

pub struct GlobalParametersBuffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    buffer_size: wgpu::BufferAddress,
    parameters_buffer: Arc<Box<wgpu::Buffer>>,

    pub tile_to_earth: Matrix4<f32>,
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
struct Globals {
    // Overlay screen info
    screen_projection: [[f32; 4]; 4],

    // Camera parameters in tile space XYZ, 1hm per unit.
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],

    // Inverted camera parameters in ecliptic XYZ, 1km per unit.
    inv_view: [[f32; 4]; 4],
    inv_proj: [[f32; 4]; 4],

    tile_to_earth: [[f32; 4]; 4],
    tile_to_earth_rotation: [[f32; 4]; 4],
    tile_to_earth_scale: [[f32; 4]; 4],
    tile_to_earth_translation: [f32; 4],
    tile_center_offset: [f32; 4],

    // Camera position in each of the above.
    camera_position_tile: [f32; 4],
    camera_position_earth_km: [f32; 4],
}

impl Globals {
    // Scale from 1:1 being full screen width to 1:1 being a letterbox, either with top-bottom
    // cutouts or left-right cutouts, depending on the aspect. This lets our screen drawing
    // routines (e.g. for text) assume that everything is undistorted, even if coordinates at
    // the edges go outside the +/- 1 range.
    pub fn with_screen_overlay_projection(mut self, gpu: &GPU) -> Self {
        let dim = gpu.physical_size();
        let aspect = gpu.aspect_ratio_f32() * 4f32 / 3f32;
        let (w, h) = if dim.width > dim.height {
            (aspect, 1f32)
        } else {
            (1f32, 1f32 / aspect)
        };
        self.screen_projection = m2v(&Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, 1f32)));
        self
    }

    // Raymarching the skybox uses the following inputs:
    //   inv_view
    //   inv_proj
    //   camera world position in kilometers
    //   sun direction vector (origin does not matter terribly much at 8 light minutes distance).
    //
    // It takes a [-1,1] fullscreen quad and turns it into worldspace vectors starting at the
    // the camera position and extending to the fullscreen quad corners, in world space.
    // Interpolation between these vectors automatically fills in one ray for every screen pixel.
    pub fn with_raymarching(mut self, camera: &ArcBallCamera) -> Self {
        let camera_position_earth_km = camera.cartesian_eye_position();
        self
    }
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
            tile_to_earth: Matrix4::identity(),
        })))
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn make_gpu_buffer(&self, globals: Globals, gpu: &GPU) -> CopyBufferDescriptor {
        let source = gpu
            .device()
            .create_buffer_mapped::<Globals>(
                1,
                wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_SRC,
            )
            .fill_from_slice(&[globals]);
        CopyBufferDescriptor::new(source, self.parameters_buffer.clone(), self.buffer_size)
    }

    pub fn make_upload_buffer(
        &self,
        camera: &ArcBallCamera,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        let mut globals: Globals = Default::default();
        let globals = globals.with_screen_overlay_projection(gpu);
        upload_buffers.push(self.make_gpu_buffer(globals, gpu));
        Ok(())
    }

    pub fn make_upload_buffer_for_arcball_on_globe(
        &self,
        camera: &ArcBallCamera,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        /*
        let globals = Self::arcball_camera_to_buffer(100f32, 100f32, 0f32, 0f32, camera, gpu);
        upload_buffers.push(self.make_gpu_buffer(globals, gpu));
        */
        Ok(())
    }

    pub fn make_upload_buffer_for_arcball_in_tile(
        &self,
        terrain: &Terrain,
        camera: &ArcBallCamera,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        /*
        let globals = Self::arcball_camera_to_buffer(
            terrain.extent_east_west_in_ft(),
            terrain.extent_north_south_in_ft(),
            terrain.origin_latitude(),
            terrain.origin_longitude(),
            camera,
            gpu,
        );
        upload_buffers.push(self.make_gpu_buffer(globals, gpu));
        */
        Ok(())
    }

    /*
    fn arcball_camera_to_buffer(
        tile_width_ft: f32,
        tile_height_ft: f32,
        tile_origin_lat_deg: f32,
        tile_origin_lon_deg: f32,
        camera: &ArcBallCamera,
        gpu: &GPU,
    ) -> Globals {
        fn deg2rad(deg: f64) -> f64 {
            deg * PI / 180.0
        }
        fn ft2hm(ft: f64) -> f64 {
            ft * FEET_TO_HM_64
        }

        let tile_width_hm = ft2hm(tile_width_ft as f64);
        let tile_height_hm = ft2hm(tile_height_ft as f64);

        let lat = deg2rad(tile_origin_lat_deg as f64);
        let lon = deg2rad(tile_origin_lon_deg as f64);

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
        let base_in_km = base * 6360f64;

        let r_lon = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0f64, -1f64, 0f64)),
            -lon,
        );
        let r_lat = UnitQuaternion::from_axis_angle(
            &Unit::new_unchecked(r_lon * Vector3::new(1f64, 0f64, 0f64)),
            -(PI / 2.0 - lat),
        );

        let tile_ul_eye = camera.eye();
        let tile_ul_tgt = camera.get_target();
        let ul_to_c = Vector3::new(tile_width_hm / 2f64, 0f64, tile_height_hm / 2f64);
        let tile_c_eye = tile_ul_eye - ul_to_c;
        let tile_c_tgt = tile_ul_tgt - ul_to_c;
        let tile_up = camera.up;

        // Create a matrix to translate between tile and earth coordinates.
        let rot_m = Matrix4::from((r_lat * r_lon).to_rotation_matrix());
        let trans_m = Matrix4::new_translation(&Vector3::new(
            base_in_km.coords[0],
            base_in_km.coords[1],
            base_in_km.coords[2],
        ));
        let scale_m = Matrix4::new_scaling(HM_TO_KM);
        let tile_to_earth = trans_m * scale_m * rot_m;

        let tile_center_offset = Vector3::new(
            tile_width_ft * FEET_TO_HM_32 / 2.0,
            0f32,
            tile_height_ft * FEET_TO_HM_32 / 2.0,
        );

        let earth_eye = tile_to_earth * tile_c_eye.to_homogeneous();
        let earth_tgt = tile_to_earth * tile_c_tgt.to_homogeneous();
        let earth_up = (tile_to_earth * tile_up.to_homogeneous()).normalize();

        let earth_view = Isometry3::look_at_rh(
            &Point3::from(earth_eye.xyz()),
            &Point3::from(earth_tgt.xyz()),
            &earth_up.xyz(),
        );

        let earth_inv_view: Matrix4<f32> = convert(earth_view.inverse().to_homogeneous());
        let earth_inv_proj: Matrix4<f32> = convert(camera.projection().inverse());

        let dim = gpu.physical_size();
        let aspect = gpu.aspect_ratio_f32() * 4f32 / 3f32;
        let (w, h) = if dim.width > dim.height {
            (aspect, 1f32)
        } else {
            (1f32, 1f32 / aspect)
        };
        Globals {
            screen_projection: m2v(&Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, 1f32))),
            view: m2v(&camera.view_matrix()),
            proj: m2v(&camera.projection_matrix()),
            inv_view: m2v(&earth_inv_view),
            inv_proj: m2v(&earth_inv_proj),
            tile_to_earth: m2v(&convert(tile_to_earth)),
            tile_to_earth_rotation: m2v(&convert(rot_m)),
            tile_to_earth_scale: m2v(&convert(scale_m)),
            tile_to_earth_translation: v2v(&convert(base_in_km.coords.to_homogeneous())),
            tile_center_offset: v2v(&tile_center_offset.to_homogeneous()),
            camera_position_tile: p2v(&convert(camera.eye())),
            camera_position_earth_km: v2v(&convert(earth_eye)),
        }
    }
    */
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
