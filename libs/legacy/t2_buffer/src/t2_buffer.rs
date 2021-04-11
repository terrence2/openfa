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
use anyhow::Result;
use catalog::Catalog;
use gpu::Gpu;
use lay::Layer;
use log::trace;
use memoffset::offset_of;
use mm::MissionMap;
use nalgebra::{Point3, Vector3};
use pal::Palette;
use physical_constants::{EARTH_RADIUS_KM_32, FEET_TO_HM_32, FEET_TO_KM};
use pic::Pic;
use std::{
    collections::{HashMap, HashSet},
    mem,
    ops::Range,
};
use t2::Terrain;
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
    pub fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        let tmp = wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float3,
                    offset: 0,
                    shader_location: 0,
                },
                // normal
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float3,
                    offset: 12,
                    shader_location: 1,
                },
                // color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float4,
                    offset: 24,
                    shader_location: 2,
                },
                // tex_coord
                wgpu::VertexAttribute {
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
    catalog: &'a Catalog,

    memo_normal: HashMap<[(u32, u32); 3], Vector3<f32>>,
    memo_position: HashMap<(u32, u32), Vector3<f32>>,
    memo_vert: HashMap<(u32, u32), Vertex>,
}

impl<'a> T2BufferFactory<'a> {
    fn new(mm: &'a MissionMap, system_palette: &'a Palette, catalog: &'a Catalog) -> Self {
        Self {
            mm,
            system_palette,
            catalog,
            memo_position: HashMap::new(),
            memo_normal: HashMap::new(),
            memo_vert: HashMap::new(),
        }
    }

    fn build(&mut self, gpu: &mut Gpu) -> Result<T2Buffer> {
        let terrain = Terrain::from_bytes(&self.catalog.read_name_sync(&self.mm.t2_name())?)?;
        let palette = self.load_palette()?;
        let (atlas, bind_group_layout, bind_group) = self.create_atlas(&palette, gpu)?;
        let (vertex_buffer, index_buffer, index_count) =
            self.upload_terrain_textured_simple(&terrain, &atlas, &palette, gpu)?;

        let mut positions = HashMap::new();
        mem::swap(&mut positions, &mut self.memo_position);

        let mut normals = HashMap::new();
        mem::swap(&mut normals, &mut self.memo_normal);

        Ok(T2Buffer {
            bind_group_layout,
            bind_group,
            vertex_buffer,
            index_buffer,
            index_count,
            positions,
            normals,
            terrain,
        })
    }

    fn load_palette(&self) -> Result<Palette> {
        let layer = Layer::from_bytes(
            &self.catalog.read_name_sync(&self.mm.layer_name())?,
            &self.system_palette,
        )?;
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
        gpu: &mut Gpu,
    ) -> Result<(TextureAtlas, wgpu::BindGroupLayout, wgpu::BindGroup)> {
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
                let data = self.catalog.read_name_sync(&name)?;
                let pic = Pic::decode(palette, &data)?;
                loaded.insert(tmap.loc.clone());
                pics.push((tmap.loc.clone(), pic));
            }
        }

        let atlas = TextureAtlas::new(pics)?;
        let image_buf = atlas.img.to_rgba8();
        let image_dim = image_buf.dimensions();
        let extent = wgpu::Extent3d {
            width: image_dim.0,
            height: image_dim.1,
            depth: 1,
        };
        let image_data = image_buf.into_raw();

        let transfer_buffer = gpu.push_buffer(
            "t2-buffer-atlas-upload",
            &image_data,
            wgpu::BufferUsage::all(),
        );
        let atlas_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("t2-buffer-atlas-texture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsage::all(),
        });
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("t2-buffer-atlas-upload-command-encoder"),
            });
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &transfer_buffer,
                layout: wgpu::TextureDataLayout {
                    offset: 0,
                    bytes_per_row: extent.width * 4,
                    rows_per_image: extent.height,
                },
            },
            wgpu::TextureCopyView {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            extent,
        );
        gpu.queue_mut().submit(vec![encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);

        let atlas_texture_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("t2-buffer-atlas-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        let sampler_resource = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("t2-atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("t2-buffer-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: true,
                                sample_type: wgpu::TextureSampleType::Uint,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler {
                                filtering: true,
                                comparison: false,
                            },
                            count: None,
                        },
                    ],
                });
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("t2-buffer-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler_resource),
                },
            ],
        });

        Ok((atlas, bind_group_layout, bind_group))
    }

    fn position_at(&mut self, terrain: &Terrain, xi: u32, zi: u32) -> Vector3<f32> {
        if let Some(v) = self.memo_position.get(&(xi, zi)) {
            return *v;
        }

        let sample = terrain.sample_at(xi, zi);

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
        let hyp_km = (d_km * d_km + EARTH_RADIUS_KM_32 * EARTH_RADIUS_KM_32).sqrt();
        let dev_km = hyp_km - EARTH_RADIUS_KM_32;
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

        let sample = terrain.sample_at(xi, zi);

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

        let mut color = palette.rgba(sample.color as usize);
        if sample.color == 0xFF {
            color[3] = 0;
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
        gpu: &Gpu,
    ) -> Result<(wgpu::Buffer, wgpu::Buffer, u32)> {
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

        let vertex_buffer = gpu.push_slice("t2-buffer-vertices", &verts, wgpu::BufferUsage::all());
        let index_buffer = gpu.push_slice("t2-buffer-indices", &indices, wgpu::BufferUsage::all());
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
        catalog: &Catalog,
        gpu: &mut Gpu,
    ) -> Result<Self> {
        trace!("T2Renderer::new");
        T2BufferFactory::new(mm, system_palette, catalog).build(gpu)
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

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.vertex_buffer.slice(..)
    }

    pub fn index_buffer(&self) -> wgpu::BufferSlice {
        self.index_buffer.slice(..)
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
    use lib::{from_dos_string, CatalogBuilder};
    use nitrous::Interpreter;
    use winit::{event_loop::EventLoop, window::Window};
    use xt::TypeManager;

    #[cfg(unix)]
    #[test]
    fn test_tile_to_earth() -> Result<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = Gpu::new(&window, Default::default(), &mut interpreter.write())?;

        let (mut catalog, inputs) =
            CatalogBuilder::build_and_select(&["FA:PALETTE.PAL".to_owned()])?;
        for &fid in &inputs {
            let label = catalog.file_label(fid)?;
            catalog.set_default_label(&label);
            let types = TypeManager::empty();
            let palette = Palette::from_bytes(&catalog.read_name_sync("PALETTE.PAL")?)?;
            let content = from_dos_string(catalog.read_name_sync("BAL.MM")?);
            let mm = MissionMap::from_str(&content, &types, &catalog)?;
            let _t2_buffer = T2Buffer::new(&mm, &palette, &catalog, &mut gpu.write())?;
        }
        Ok(())
    }
}
