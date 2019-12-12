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
use crate::texture_atlas::TextureAtlas;
use failure::Fallible;
use gpu::GPU;
use lay::Layer;
use lib::Library;
use log::trace;
use memoffset::offset_of;
use mm::MissionMap;
use nalgebra::{Point3, Vector3};
use pal::Palette;
use pic::Pic;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    mem,
    ops::Range,
    sync::Arc,
};
use t2::{Sample, Terrain};
use universe::{EARTH_RADIUS_KM, FEET_TO_HM_32, FEET_TO_KM};
use wgpu;
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
pub struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    tex_coord: [f32; 2],
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
                // color
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float4,
                    offset: 24,
                    shader_location: 2,
                },
                // tex_coord
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float2,
                    offset: 40,
                    shader_location: 3,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(Vertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(Vertex, normal) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(Vertex, color) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[3].offset,
            offset_of!(Vertex, tex_coord) as wgpu::BufferAddress
        );

        tmp
    }
}

// Hold our working state.
struct T2BufferFactory<'a> {
    mm: &'a MissionMap,
    system_palette: &'a Palette,
    lib: &'a Library,

    memo_normal: HashMap<[(u32, u32); 3], Vector3<f32>>,
    memo_position: HashMap<(u32, u32), Vector3<f32>>,
    memo_vert: HashMap<(u32, u32), Vertex>,
}

impl<'a> T2BufferFactory<'a> {
    fn new(mm: &'a MissionMap, system_palette: &'a Palette, lib: &'a Library) -> Self {
        Self {
            mm,
            system_palette,
            lib,
            memo_position: HashMap::new(),
            memo_normal: HashMap::new(),
            memo_vert: HashMap::new(),
        }
    }

    fn build(&mut self, gpu: &mut GPU) -> Fallible<Arc<RefCell<T2Buffer>>> {
        let terrain = Terrain::from_bytes(&self.lib.load(&self.mm.t2_name())?)?;
        let palette = self.load_palette()?;
        let (atlas, bind_group_layout, bind_group) = self.create_atlas(&palette, gpu)?;
        let (vertex_buffer, index_buffer, index_count) =
            self.upload_terrain_textured_simple(&terrain, &atlas, &palette, gpu.device())?;

        let mut positions = HashMap::new();
        mem::swap(&mut positions, &mut self.memo_position);

        let mut normals = HashMap::new();
        mem::swap(&mut normals, &mut self.memo_normal);

        Ok(Arc::new(RefCell::new(T2Buffer {
            bind_group_layout,
            bind_group,
            vertex_buffer,
            index_buffer,
            index_count,
            positions,
            normals,
            terrain,
        })))
    }

    fn load_palette(&self) -> Fallible<Palette> {
        let layer = Layer::from_bytes(&self.lib.load(&self.mm.layer_name())?, self.lib)?;
        let layer_index = if self.mm.layer_index() != 0 {
            self.mm.layer_index()
        } else {
            2
        };

        let layer_data = layer.for_index(layer_index)?;
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
        let mut palette = self.system_palette.clone();
        palette.overlay_at(&r1, 0xF0 - 1)?;
        palette.overlay_at(&r0, 0xE0 - 1)?;
        palette.overlay_at(&r3, 0xD0)?;
        palette.overlay_at(&r2, 0xC0)?;

        Ok(palette)
    }

    // Texture counts for all FA T2's.
    // APA: 68 x 256 (6815744 texels)
    // BAL: 66 x 256
    // CUB: 66 x 256
    // EGY: 49 x 256
    // FRA: 47 x 256
    // GRE: 68
    // IRA: 51 x 256
    // KURILE: 236 (Kxxxxxx) x 128/256 (33554432 texels)
    // LFA: 68
    // NSK: 68
    // PGU: 51
    // SPA: 49
    // TVIET: 42 (TVI) x 256
    // UKR: 29
    // VLA: 52
    // WTA: 68
    fn create_atlas(
        &self,
        palette: &Palette,
        gpu: &mut GPU,
    ) -> Fallible<(TextureAtlas, wgpu::BindGroupLayout, wgpu::BindGroup)> {
        // Load all images with our custom palette.
        let mut pics = Vec::new();
        {
            let mut loaded = HashSet::new();
            let texture_base_name = self.mm.get_base_texture_name()?;
            for tmap in self.mm.texture_maps() {
                if loaded.contains(&tmap.loc) {
                    continue;
                }
                let name = tmap.loc.pic_file(&texture_base_name);
                let data = self.lib.load(&name)?;
                let pic = Pic::decode(palette, &data)?;
                loaded.insert(tmap.loc.clone());
                pics.push((tmap.loc.clone(), pic));
            }
        }

        let atlas = TextureAtlas::new(pics)?;
        let image_buf = atlas.img.to_rgba();
        let image_dim = image_buf.dimensions();
        let extent = wgpu::Extent3d {
            width: image_dim.0,
            height: image_dim.1,
            depth: 1,
        };
        let image_data = image_buf.into_raw();

        let transfer_buffer = gpu
            .device()
            .create_buffer_mapped(image_data.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&image_data);
        let atlas_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            size: extent,
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsage::all(),
        });
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &transfer_buffer,
                offset: 0,
                row_pitch: extent.width * 4,
                image_height: extent.height,
            },
            wgpu::TextureCopyView {
                texture: &atlas_texture,
                mip_level: 0,
                array_layer: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            extent,
        );
        gpu.queue_mut().submit(&[encoder.finish()]);
        gpu.device().poll(true);

        let atlas_texture_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::Rgba8Unorm,
            dimension: wgpu::TextureViewDimension::D2,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1, // mip level
            base_array_layer: 0,
            array_layer_count: 1,
        });
        let sampler_resource = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare_function: wgpu::CompareFunction::Never,
        });

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    bindings: &[
                        wgpu::BindGroupLayoutBinding {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                multisampled: true,
                                dimension: wgpu::TextureViewDimension::D2,
                            },
                        },
                        wgpu::BindGroupLayoutBinding {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler,
                        },
                    ],
                });
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler_resource),
                },
            ],
        });

        Ok((atlas, bind_group_layout, bind_group))
    }

    fn sample_at(terrain: &Terrain, xi: u32, zi: u32) -> Sample {
        let offset = (zi * terrain.width() + xi) as usize;
        if offset < terrain.samples.len() {
            terrain.samples[offset]
        } else {
            let offset = ((zi - 1) * terrain.width() + xi) as usize;
            if offset < terrain.samples.len() {
                terrain.samples[offset]
            } else {
                let offset = ((zi - 1) * terrain.width() + (xi - 1)) as usize;
                terrain.samples[offset]
            }
        }
    }

    fn position_at(&mut self, terrain: &Terrain, xi: u32, zi: u32) -> Vector3<f32> {
        if let Some(v) = self.memo_position.get(&(xi, zi)) {
            return *v;
        }

        let sample = Self::sample_at(terrain, xi, zi);

        let xf = xi as f32 / terrain.width() as f32;
        let zf = zi as f32 / terrain.height() as f32;
        let scale_x_ft = terrain.extent_east_west_in_ft();
        let scale_z_ft = terrain.extent_north_south_in_ft();
        let x_hm = xf * scale_x_ft * FEET_TO_HM_32;
        let z_hm = (1f32 - zf) * scale_z_ft * FEET_TO_HM_32;
        let mut h = -f32::from(sample.height) * 3f32; /*/ 512f32 + 0.1f32*/

        // Compute distance from center.
        let center_x_km = scale_x_ft * FEET_TO_KM / 2f32;
        let center_z_km = scale_z_ft * FEET_TO_KM / 2f32;
        let x_km = xf * scale_x_ft * FEET_TO_KM;
        let z_km = zf * scale_z_ft * FEET_TO_KM;
        let xc = (center_x_km - x_km).abs();
        let zc = (center_z_km - z_km).abs();
        let d_km = (xc * xc + zc * zc).sqrt();
        let hyp_km = (d_km * d_km + EARTH_RADIUS_KM * EARTH_RADIUS_KM).sqrt();
        let dev_km = hyp_km - EARTH_RADIUS_KM;
        h += dev_km * 10f32;

        let position = Vector3::new(x_hm, h, z_hm);
        self.memo_position.insert((xi, zi), position);
        position
    }

    fn normal_for(&mut self, terrain: &Terrain, coords: [(u32, u32); 3]) -> Vector3<f32> {
        if let Some(&normal) = self.memo_normal.get(&coords) {
            return normal;
        }
        if coords[0] == coords[1] || coords[1] == coords[2] || coords[0] == coords[2] {
            let normal = Vector3::new(0f32, -1f32, 0f32);
            self.memo_normal.insert(coords, normal);
            return normal;
        }
        let p0 = self.position_at(terrain, coords[0].0, coords[0].1);
        let p1 = self.position_at(terrain, coords[1].0, coords[1].1);
        let p2 = self.position_at(terrain, coords[2].0, coords[2].1);
        let normal = (p2 - p1).cross(&(p0 - p1)).normalize();
        self.memo_normal.insert(coords, normal);
        normal
    }

    // The T2 are flat and square. And cover several degrees of the earth. That means we need to
    // actually account for curvature in a reasonable way. Flightgear handles this by having tile
    // coordinates in lat/lon/asl. This (probably) won't work because MM files list shapes to put
    // on the map in feet off of the origin. If we change the value of up, we need to rotate
    // all the shapes. Some of the shapes need to line up closely to make sense, like runways.
    //
    // We deal with this by draping down in the direction of the tile, rather than towards
    // earth center, and using the result as the lat-lon. e.g. we treat XYZ as primary, but
    //
    fn compute_at(
        &mut self,
        terrain: &Terrain,
        palette: &Palette,
        xi: u32,
        zi: u32,
        tex_coord: [f32; 2],
        verts: &mut Vec<Vertex>,
    ) {
        if let Some(v) = self.memo_vert.get(&(xi, zi)) {
            let mut vert = *v;
            vert.tex_coord = tex_coord;
            verts.push(vert);
            return;
        }

        let sample = Self::sample_at(terrain, xi, zi);

        let x0 = xi.saturating_sub(1);
        let x1 = xi;
        let x2 = (xi + 1).min(terrain.width() - 1);
        let z0 = zi.saturating_sub(1);
        let z1 = zi;
        let z2 = (zi + 1).min(terrain.height() - 1);
        let p11 = self.position_at(terrain, x1, z1);
        let normals = [
            self.normal_for(terrain, [(x0, z1), (x1, z1), (x0, z0)]),
            self.normal_for(terrain, [(x0, z0), (x1, z1), (x1, z0)]),
            self.normal_for(terrain, [(x0, z2), (x1, z2), (x0, z1)]),
            self.normal_for(terrain, [(x0, z1), (x1, z2), (x1, z1)]),
            self.normal_for(terrain, [(x1, z1), (x2, z1), (x1, z0)]),
            self.normal_for(terrain, [(x1, z0), (x2, z1), (x2, z0)]),
            self.normal_for(terrain, [(x1, z2), (x2, z2), (x1, z1)]),
            self.normal_for(terrain, [(x1, z1), (x2, z2), (x2, z1)]),
        ];
        let mut normal = Vector3::identity();
        for n in &normals {
            normal += n;
        }
        let normal = normal.normalize();

        let mut color = palette.rgba(sample.color as usize).unwrap();
        if sample.color == 0xFF {
            color.data[3] = 0;
        }

        let vert = Vertex {
            position: [p11[0], p11[1], p11[2]],
            normal: [normal[0], normal[1], normal[2]],
            color: [
                f32::from(color[0]) / 255f32,
                f32::from(color[1]) / 255f32,
                f32::from(color[2]) / 255f32,
                f32::from(color[3]) / 255f32,
            ],
            tex_coord,
        };
        self.memo_vert.insert((xi, zi), vert);
        verts.push(vert);
    }

    fn upload_terrain_textured_simple(
        &mut self,
        terrain: &Terrain,
        atlas: &TextureAtlas,
        palette: &Palette,
        device: &wgpu::Device,
    ) -> Fallible<(wgpu::Buffer, wgpu::Buffer, u32)> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        // Each patch has a fixed strip pattern.
        let mut patch_indices = Vec::new();
        for row in 0..4 {
            let row_off = row * 5;

            patch_indices.push(row_off);
            patch_indices.push(row_off);

            for column in 0..5 {
                patch_indices.push(row_off + column);
                patch_indices.push(row_off + column + 5);
            }

            patch_indices.push(row_off + 4 + 5);
            patch_indices.push(row_off + 4 + 5);
        }
        let push_patch_indices = |base: u32, indices: &mut Vec<u32>| {
            for pi in &patch_indices {
                indices.push(base + *pi);
            }
        };

        for zi_base in (0..terrain.height()).step_by(4) {
            for xi_base in (0..terrain.width()).step_by(4) {
                let base = verts.len() as u32;

                // Upload one patch of vertices, possibly with a texture.
                let frame_info = self
                    .mm
                    .texture_map(xi_base, zi_base)
                    .map(|tmap| (&atlas.frames[&tmap.loc], &tmap.orientation));
                for z_off in 0..=4 {
                    for x_off in 0..=4 {
                        let zi = zi_base + z_off;
                        let xi = xi_base + x_off;

                        let tex_coord = frame_info
                            .map(|(frame, orientation)| {
                                frame.interp(x_off as f32 / 4f32, z_off as f32 / 4f32, orientation)
                            })
                            .unwrap_or([0f32, 0f32]);

                        self.compute_at(terrain, palette, xi, zi, tex_coord, &mut verts);
                    }
                }
                push_patch_indices(base, &mut indices);
            }
        }

        trace!(
            "uploading vertex buffer with {} bytes",
            std::mem::size_of::<Vertex>() * verts.len()
        );
        let vertex_buffer = device
            .create_buffer_mapped(verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&verts);

        trace!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let index_buffer = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&indices);

        Ok((vertex_buffer, index_buffer, indices.len() as u32))
    }
}

pub struct T2Buffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,

    // We need access to the height data for collisions, layout, etc.
    positions: HashMap<(u32, u32), Vector3<f32>>,
    normals: HashMap<[(u32, u32); 3], Vector3<f32>>,
    terrain: Terrain,
}

impl T2Buffer {
    pub fn new(
        mm: &MissionMap,
        system_palette: &Palette,
        lib: &Library,
        gpu: &mut GPU,
    ) -> Fallible<Arc<RefCell<Self>>> {
        trace!("T2Renderer::new");
        T2BufferFactory::new(mm, system_palette, lib).build(gpu)
    }

    pub fn t2(&self) -> &Terrain {
        &self.terrain
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    pub fn index_buffer(&self) -> &wgpu::Buffer {
        &self.index_buffer
    }

    pub fn index_range(&self) -> Range<u32> {
        0..self.index_count
    }

    #[allow(clippy::many_single_char_names)]
    pub fn ground_height_at_tile(&self, p: &Point3<f32>) -> f32 {
        let scale_x_hm = self.terrain.extent_east_west_in_ft() * FEET_TO_HM_32;
        let scale_z_hm = self.terrain.extent_north_south_in_ft() * FEET_TO_HM_32;

        // The terrain is draped, so p.x/z do not need projection.
        if p.coords[0] < 0.0
            || p.coords[2] < 0.0
            || p.coords[0] > scale_x_hm
            || p.coords[1] > scale_z_hm
        {
            return 0f32;
        }

        let xf = p.coords[0] / scale_x_hm;
        let zf = 1f32 - (p.coords[2] / scale_z_hm);
        let x0 = (xf * self.terrain.width() as f32) as u32;
        let z0 = (zf * self.terrain.height() as f32) as u32;
        let x1 = x0 + 1;
        let z1 = z0 + 1;
        if x1 >= self.terrain.width() || z1 >= self.terrain.height() {
            return 0f32;
        }

        let swi = (x0, z1);
        let sei = (x1, z1);
        let nwi = (x0, z0);
        let nei = (x1, z0);
        let sw = self.positions[&swi];
        let se = self.positions[&sei];
        let nw = self.positions[&nwi];
        let ne = self.positions[&nei];
        assert!(p.coords[0] >= sw[0]);
        assert!(p.coords[0] <= ne[0]);
        assert!(p.coords[0] <= se[0]);
        assert!(p.coords[0] >= nw[0]);
        assert!(p.coords[2] >= sw[2]);
        assert!(p.coords[2] <= ne[2]);
        assert!(p.coords[2] >= se[2]);
        assert!(p.coords[2] <= nw[2]);

        // For upper left tris: nw, ne, se
        let down = Vector3::new(0f32, 1f32, 0f32);
        let norm = self.normals[&[nwi, sei, nei]];
        let d = ((nw - p.coords).dot(&norm)) / down.dot(&norm);
        let p1 = p + down * d;

        // Find out if we actually computed the correct triangle.
        let w = scale_x_hm / self.terrain.width() as f32;
        let h = scale_z_hm / self.terrain.height() as f32;
        let x = p1[0] - ne[0];
        let y = p1[2] - sw[2];
        if w * h > w * y + h * x {
            // For lower right tris: nw, se, sw
            let norm = self.normals[&[swi, sei, nwi]];
            let d = ((sw - p.coords).dot(&norm)) / down.dot(&norm);
            let p2 = p + down * d;
            p2[1]
        } else {
            p1[1]
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use input::InputSystem;
    use omnilib::OmniLib;
    use xt::TypeManager;

    #[test]
    fn test_tile_to_earth() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let omni = OmniLib::new_for_test_in_games(&["FA"])?;
        let lib = omni.library("FA");
        let types = TypeManager::new(lib.clone());
        let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;
        let mm = MissionMap::from_str(&lib.load_text("BAL.MM")?, &types, &lib)?;
        let _t2_buffer = T2Buffer::new(&mm, &palette, &lib, &mut gpu)?;
        Ok(())
    }
}
