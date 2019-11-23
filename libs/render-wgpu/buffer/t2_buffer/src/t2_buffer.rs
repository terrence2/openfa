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
use pal::Palette;
use pic::Pic;
use std::{cell::RefCell, collections::HashSet, mem, ops::Range, sync::Arc};
use t2::Terrain;
use universe::FEET_TO_HM;
use wgpu;

#[derive(Copy, Clone, Default)]
pub struct Vertex {
    position: [f32; 3],
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
                // color
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float4,
                    offset: 12,
                    shader_location: 1,
                },
                // tex_coord
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float2,
                    offset: 28,
                    shader_location: 2,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(Vertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(Vertex, color) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[2].offset,
            offset_of!(Vertex, tex_coord) as wgpu::BufferAddress
        );

        tmp
    }
}

pub struct T2Buffer {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,

    // We need access to the height data for collisions, layout, etc.
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

        let terrain = Terrain::from_bytes(&lib.load(&mm.t2_name())?)?;
        let palette = Self::load_palette(&mm, system_palette, lib)?;
        let (atlas, bind_group_layout, bind_group) = Self::create_atlas(&mm, &palette, &lib, gpu)?;
        let (vertex_buffer, index_buffer, index_count) =
            Self::upload_terrain_textured_simple(&mm, &terrain, &atlas, &palette, gpu.device())?;

        Ok(Arc::new(RefCell::new(Self {
            bind_group_layout,
            bind_group,
            vertex_buffer,
            index_buffer,
            index_count,
            terrain,
        })))
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

    fn load_palette(mm: &MissionMap, system_palette: &Palette, lib: &Library) -> Fallible<Palette> {
        let layer = Layer::from_bytes(&lib.load(&mm.layer_name())?, &lib)?;
        let layer_index = if mm.layer_index() != 0 {
            mm.layer_index()
        } else {
            2
        };

        let layer_data = layer.for_index(layer_index)?;
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
        let mut palette = system_palette.clone();
        palette.overlay_at(&r1, 0xF0 - 1)?;
        palette.overlay_at(&r0, 0xE0 - 1)?;
        palette.overlay_at(&r3, 0xD0)?;
        palette.overlay_at(&r2, 0xC0)?;

        //palette.override_one(0xFE, [0, 0, 0]);

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
        mm: &MissionMap,
        palette: &Palette,
        lib: &Library,
        gpu: &mut GPU,
    ) -> Fallible<(TextureAtlas, wgpu::BindGroupLayout, wgpu::BindGroup)> {
        // Load all images with our custom palette.
        let mut pics = Vec::new();
        {
            let mut loaded = HashSet::new();
            let texture_base_name = mm.get_base_texture_name()?;
            for tmap in mm.texture_maps() {
                if loaded.contains(&tmap.loc) {
                    continue;
                }
                let name = tmap.loc.pic_file(&texture_base_name);
                let data = lib.load(&name)?;
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

    fn sample_at(terrain: &Terrain, palette: &Palette, xi: u32, zi: u32) -> ([f32; 3], [f32; 4]) {
        let offset = (zi * terrain.width + xi) as usize;
        let sample = if offset < terrain.samples.len() {
            terrain.samples[offset]
        } else {
            let offset = ((zi - 1) * terrain.width + xi) as usize;
            if offset < terrain.samples.len() {
                terrain.samples[offset]
            } else {
                let offset = ((zi - 1) * terrain.width + (xi - 1)) as usize;
                terrain.samples[offset]
            }
        };

        let xf = xi as f32 / terrain.width as f32;
        let zf = zi as f32 / terrain.height as f32;
        let scale_x = terrain.extent_east_west_in_ft();
        let scale_z = terrain.extent_north_south_in_ft();
        let x = xf * scale_x * FEET_TO_HM;
        let z = (1f32 - zf) * scale_z * FEET_TO_HM;
        let h = -f32::from(sample.height) * 2f32; /*/ 512f32 + 0.1f32*/

        let mut color = palette.rgba(sample.color as usize).unwrap();
        // FIXME: re-hide water once we get this positioned right.
        if sample.color == 0xFF {
            color.data[3] = 0;
        }

        (
            [x, h, z],
            [
                f32::from(color[0]) / 255f32,
                f32::from(color[1]) / 255f32,
                f32::from(color[2]) / 255f32,
                f32::from(color[3]) / 255f32,
            ],
        )
    }

    fn upload_terrain_textured_simple(
        mm: &MissionMap,
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

        for zi_base in (0..terrain.height).step_by(4) {
            for xi_base in (0..terrain.width).step_by(4) {
                let base = verts.len() as u32;

                // Upload one patch of vertices, possibly with a texture.
                let frame_info = mm
                    .texture_map(xi_base, zi_base)
                    .map(|tmap| (&atlas.frames[&tmap.loc], &tmap.orientation));
                for z_off in 0..=4 {
                    for x_off in 0..=4 {
                        let zi = zi_base + z_off;
                        let xi = xi_base + x_off;
                        let (position, color) = Self::sample_at(terrain, palette, xi, zi);

                        verts.push(Vertex {
                            position,
                            color,
                            tex_coord: frame_info
                                .map(|(frame, orientation)| {
                                    frame.interp(
                                        x_off as f32 / 4f32,
                                        z_off as f32 / 4f32,
                                        orientation,
                                    )
                                })
                                .unwrap_or([0f32, 0f32]),
                        });
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
