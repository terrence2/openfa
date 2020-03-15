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
use absolute_unit::{Kilometers, Radians};
use camera::ArcBallCamera;
use failure::Fallible;
use frame_graph::CopyBufferDescriptor;
use geodesy::{Cartesian, GeoCenter, Graticule};
use geometry::{algorithm::solid_angle, IcoSphere, Plane};
use gpu::GPU;
use memoffset::offset_of;
use nalgebra::{Point3, Vector3};
use std::{
    cell::RefCell,
    cmp::{Ord, Ordering},
    collections::BinaryHeap,
    mem,
    ops::Range,
    sync::Arc,
};
use wgpu;
use zerocopy::{AsBytes, FromBytes};

const EARTH_TO_KM: f64 = 6370.0;
const EVEREST_TO_KM: f64 = 8.848_039_2;

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct PatchVertex {
    position: [f32; 3],
    normal: [f32; 3],
    graticule: [f32; 2],
}

impl PatchVertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 0,
                    shader_location: 0,
                },
                // normal
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float3,
                    offset: 12,
                    shader_location: 1,
                },
                // graticule
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float2,
                    offset: 24,
                    shader_location: 2,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(PatchVertex, position) as wgpu::BufferAddress
        );

        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(PatchVertex, normal) as wgpu::BufferAddress
        );

        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(PatchVertex, graticule) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<PatchVertex>(), 32);

        tmp
    }
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct Vertex {
    position: [f32; 4],
}

impl Vertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float4,
                    offset: 0,
                    shader_location: 0,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(Vertex, position) as wgpu::BufferAddress
        );

        assert_eq!(mem::size_of::<Vertex>(), 16);

        tmp
    }
}

#[derive(Debug, Copy, Clone)]
struct PatchInfo {
    level: usize,
    solid_angle: f64,
    normal: Vector3<f64>,
    goodness: f64,

    // In geocentric, cartesian kilometers
    pts: [Point3<f64>; 3],
}

impl PatchInfo {
    fn new(
        level: usize,
        eye_position: &Point3<f64>,
        eye_direction: &Vector3<f64>,
        pts: [Point3<f64>; 3],
    ) -> Self {
        let solid_angle = solid_angle(&eye_position, &eye_direction, &pts);
        let normal = (pts[1].coords - pts[0].coords)
            .cross(&(pts[2].coords - pts[0].coords))
            .normalize();
        Self {
            level,
            solid_angle,
            normal,
            goodness: solid_angle,
            pts,
        }
    }

    fn is_behind_plane(&self, plane: &Plane<f64>) -> bool {
        // Patch Extent:
        //   outer: the three planes cutting from geocenter through each pair of points in vertices.
        //   bottom: radius of the planet
        //   top: radius of planet from height of everest

        // Two phases:
        //   1) Convex hull over points
        //   2) Plane-sphere for convex top area

        // bottom points
        for p in &self.pts {
            //println!("    d: {}, {}", plane.distance(&p), plane.d());
            if plane.distance(&p) >= 0.0 {
                return false;
            }
        }
        for p in &self.pts {
            let top_point = p + (p.coords.normalize() * EVEREST_TO_KM);
            println!("    p: {}", p);
            println!("   tp: {}", top_point);
            println!(
                "    d: {} vs {} / d: {}",
                plane.distance(&top_point),
                plane.distance(&p),
                plane.d()
            );
            if plane.distance(&top_point) >= 0.0 {
                return false;
            }
        }

        // bottom sphere -> (0,0,0)xEARTH
        // top sphere

        return true;
    }

    fn is_back_facing(&self, eye_position: &Point3<f64>) -> bool {
        for p in &self.pts {
            if (p - eye_position).dot(&self.normal) <= -0.00001f64 {
                return false;
            }
        }
        return true;
    }

    fn keep(
        &self,
        horizon_plane: &Plane<f64>,
        eye_direction: &Vector3<f64>,
        eye_position: &Point3<f64>,
    ) -> bool {
        // Cull back-facing
        /*
        if self.is_back_facing(eye_position) {
            println!("  no - back facing");
            return false;
        }
        */

        // Cull below horizon
        if self.is_behind_plane(&horizon_plane) {
            println!("  no - below horizon");
            return false;
        }

        // TODO: Cull outside the view frustum

        return true;
    }
}

impl Eq for PatchInfo {}

impl PartialEq for PatchInfo {
    fn eq(&self, other: &Self) -> bool {
        self.goodness == other.goodness
    }
}

impl PartialOrd for PatchInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.goodness.partial_cmp(&other.goodness)
    }
}
impl Ord for PatchInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Less)
    }
}

pub struct TerrainGeoBuffer {
    // bind_group_layout: wgpu::BindGroupLayout,
    // bind_group: wgpu::BindGroup,
    sphere: IcoSphere,

    num_patches: usize,
    patch_vertex_buffer: Arc<Box<wgpu::Buffer>>,
    patch_index_buffer: wgpu::Buffer,
}

impl TerrainGeoBuffer {
    pub fn new(
        num_patches: usize,
        _gen_subdivisions: usize,
        device: &wgpu::Device,
    ) -> Fallible<Arc<RefCell<Self>>> {
        /*
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[wgpu::BindGroupLayoutBinding {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
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
                    buffer: &params_buffer,
                    range: 0..buffer_size,
                },
            }],
        });
        */

        println!(
            "patch_vertex_buffer: {:08X}",
            mem::size_of::<PatchVertex>() * 3 * num_patches
        );
        let patch_vertex_buffer =
            Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
                size: (mem::size_of::<PatchVertex>() * 3 * num_patches) as wgpu::BufferAddress,
                usage: wgpu::BufferUsage::all(),
            })));

        let mut patch_indices = Vec::new();
        patch_indices.push(0u32);
        patch_indices.push(1u32);
        patch_indices.push(1u32);
        patch_indices.push(2u32);
        patch_indices.push(2u32);
        patch_indices.push(0u32);
        let patch_index_buffer = device
            .create_buffer_mapped(patch_indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&patch_indices);

        Ok(Arc::new(RefCell::new(Self {
            sphere: IcoSphere::new(0),

            num_patches,
            patch_vertex_buffer,
            patch_index_buffer,
        })))
    }

    pub fn make_upload_buffer(
        &self,
        camera: &ArcBallCamera,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        use std::time::Instant;
        let loop_start = Instant::now();

        let camera_target = camera.cartesian_target_position::<Kilometers>().vec64();
        let eye_position = camera.cartesian_eye_position::<Kilometers>().point64();
        let eye_direction = camera_target - eye_position.coords;

        let horizon_plane = Plane::from_normal_and_distance(
            eye_position.coords.normalize(),
            (EARTH_TO_KM * EARTH_TO_KM) / eye_position.coords.magnitude(),
        );
        println!("horizon: {:?}", horizon_plane);

        let loop_start = Instant::now();
        let mut patches = BinaryHeap::with_capacity(self.num_patches);
        for (i, face) in self.sphere.faces.iter().enumerate() {
            let v0 = Point3::from(self.sphere.verts[face.i0()] * EARTH_TO_KM);
            let v1 = Point3::from(self.sphere.verts[face.i1()] * EARTH_TO_KM);
            let v2 = Point3::from(self.sphere.verts[face.i2()] * EARTH_TO_KM);
            let patch = PatchInfo::new(0, &eye_position, &eye_direction, [v0, v1, v2]);

            println!("Checking {}: ", i);
            if patch.keep(&horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch);
            }
        }
        let elapsed = Instant::now() - loop_start;
        println!(
            "lvl0: {:?}, {:?}us per iteration - {} patches",
            elapsed,
            elapsed.as_micros() / self.sphere.faces.len() as u128,
            patches.len(),
        );

        // Split patches until we have an optimal equal-area partitioning.
        /*
        let loop_start = Instant::now();
        while patches.len() > 0 && patches.len() < self.num_patches - 4 {
            let patch = patches.pop().unwrap();
            let [v0, v1, v2] = patch.pts;
            let a = Point3::from(
                IcoSphere::bisect_edge(&v0.coords, &v1.coords).normalize() * EARTH_TO_KM,
            );
            let b = Point3::from(
                IcoSphere::bisect_edge(&v1.coords, &v2.coords).normalize() * EARTH_TO_KM,
            );
            let c = Point3::from(
                IcoSphere::bisect_edge(&v2.coords, &v0.coords).normalize() * EARTH_TO_KM,
            );

            let patch0 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [v0, a, c]);
            let patch1 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [v1, b, a]);
            let patch2 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [v2, c, b]);
            let patch3 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [a, b, c]);

            if patch0.keep(&horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch0);
            }
            if patch1.keep(&horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch1);
            }
            if patch2.keep(&horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch2);
            }
            if patch3.keep(&horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch3);
            }
        }
        println!("split: {:?}", Instant::now() - loop_start);
        */

        let loop_start = Instant::now();
        let mut verts = Vec::with_capacity(3 * self.num_patches);
        for patch in &patches {
            let [v0, v1, v2] = patch.pts;
            let n0 = v0.coords.normalize();
            let n1 = v1.coords.normalize();
            let n2 = v2.coords.normalize();
            verts.push(PatchVertex {
                position: [v0[0] as f32, v0[1] as f32, v0[2] as f32],
                normal: [n0[0] as f32, n0[1] as f32, n0[2] as f32],
                graticule: Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(
                    v0,
                ))
                .lat_lon::<Radians, f32>(),
            });
            verts.push(PatchVertex {
                position: [v1[0] as f32, v1[1] as f32, v1[2] as f32],
                normal: [n1[0] as f32, n1[1] as f32, n1[2] as f32],
                graticule: Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(
                    v1,
                ))
                .lat_lon::<Radians, f32>(),
            });
            verts.push(PatchVertex {
                position: [v2[0] as f32, v2[1] as f32, v2[2] as f32],
                normal: [n2[0] as f32, n2[1] as f32, n2[2] as f32],
                graticule: Graticule::<GeoCenter>::from(Cartesian::<GeoCenter, Kilometers>::from(
                    v2,
                ))
                .lat_lon::<Radians, f32>(),
            });
        }
        println!("verts: {:?}", Instant::now() - loop_start);
        let loop_start = Instant::now();

        while verts.len() < 3 * self.num_patches {
            verts.push(PatchVertex {
                position: [0f32; 3],
                normal: [0f32; 3],
                graticule: [0f32; 2],
            });
        }
        let vertex_buffer = gpu
            .device()
            .create_buffer_mapped(verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&verts);
        upload_buffers.push(CopyBufferDescriptor::new(
            vertex_buffer,
            self.patch_vertex_buffer.clone(),
            (mem::size_of::<PatchVertex>() * verts.len()) as wgpu::BufferAddress,
        ));

        println!("dt: {:?}", Instant::now() - loop_start);
        Ok(())
    }

    /*
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
    pub fn block_bind_group(&self) -> &wgpu::BindGroup {
        &self.block_bind_group
    }
    */

    pub fn num_patches(&self) -> i32 {
        self.num_patches as i32
    }

    pub fn patch_index_buffer(&self) -> &wgpu::Buffer {
        &self.patch_index_buffer
    }

    pub fn patch_vertex_buffer(&self) -> &wgpu::Buffer {
        &self.patch_vertex_buffer
    }

    pub fn patch_index_range(&self) -> Range<u32> {
        0..6
    }
}
