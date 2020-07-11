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

// Data Sets:
//   NASA's Shuttle Radar Topography Map (SRTM); height data
//
// Desired Data Sets:
//   NASA's Blue Marble Next Generation (BMNG); diffuse color information
//   JAXA's Advanced Land Observing Satellite "DAICHI" (ALOS); height data
//   Something cartesian polar north and south
//
// Each data set gets a directory.
// That directory has an index.json with all of the tile metadata in it.
//   Data sets may have spherical or cartesian-polar coordinates.
//   Data sets may contain height or color data.
// That directory contains one subdir for every available resolution.
// All units for spherical data-sets are in arcseconds, including resolution above.
// Tiles are 512x512 with a one pixel overlap with other tiles to enable linear filtering. Data is
//   stored row-major with low indexed rows to the south, going north and low index.
//
// Tile cache design:
//   Upload one (or more) mega-texture(s) for each dataset.
//   The index is a fixed, large texture:
//     * SRTM has 1' resolution, but tiles have at minimum 510' of content.
//     * We need a (360|180 * 60 * 60 / 510) pixels wide|high texture => 2541.17 x 1270.59
//     * 2560 * 1280 px index texture.
//     * Open Question: do we have data sets with higher resolution that we want to support? Will
//       those inherently load in larger blocks to support the above index scheme?
//     * Open Question: one index per dataset or shared globally and we assume the same resolution
//       choice for all datasets?
//   Tile Updates:
//     * The patch tree "votes" on what resolution it wants.
//     * We select a handful of the most needed that are not present to upload and create copy ops.
//     * We update the index texture with a compute shader that overwrites if the scale is smaller.

// First pass: hard code everything.
use crate::{tile::QuadTree, GpuDetail};
use failure::Fallible;
use geodesy::{GeoCenter, Graticule};
use gpu::GPU;
use std::{fs::File, io::Read, path::PathBuf};

const TILE_SIZE: u32 = 512;

const INDEX_WIDTH: u32 = 2560;
const INDEX_HEIGHT: u32 = 1280;

pub(crate) struct TileManager {
    #[allow(unused)]
    srtm_index_texture_extent: wgpu::Extent3d,
    #[allow(unused)]
    srtm_index_texture: wgpu::Texture,
    #[allow(unused)]
    srtm_index_texture_view: wgpu::TextureView,
    #[allow(unused)]
    srtm_index_texture_sampler: wgpu::Sampler,

    #[allow(unused)]
    srtm_atlas_texture_extent: wgpu::Extent3d,
    #[allow(unused)]
    srtm_atlas_texture: wgpu::Texture,
    #[allow(unused)]
    srtm_atlas_texture_view: wgpu::TextureView,
    #[allow(unused)]
    srtm_atlas_texture_sampler: wgpu::Sampler,

    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,

    tree: QuadTree,
}

impl TileManager {
    pub(crate) fn new(gpu: &mut GPU, gpu_detail: &GpuDetail) -> Fallible<Self> {
        let srtm_path = PathBuf::from("/home/terrence/storage/srtm/output/srtm/");

        // FIXME: abstract this out into a DataSet container of some sort so we can at least
        //        get rid of the extremely long names.

        // The index texture is just a more or less normal texture. The longitude in spherical
        // coordinates maps to `s` and the latitude maps to `t` (with some important finagling).
        // Each pixel of the index is arranged such that it maps to a single tile at highest
        // resolution: 30 arcseconds per sample at 510 samples. Lower resolution tiles, naturally
        // fill more than a single pixel of the index. We sample the index texture with "nearest"
        // filtering such that any sample taken in the tile area will map exactly to the right
        // tile. Tiles are additionally fringed with a border such that linear filtering can be
        // used in the tile lookup without further effort. In combination, this lets us point the
        // full power of the texturing hardware at the problem, with very little extra overhead.
        let srtm_index_texture_extent = wgpu::Extent3d {
            width: INDEX_WIDTH,
            height: INDEX_HEIGHT,
            depth: 1,
        };
        let srtm_index_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-index-texture"),
            size: srtm_index_texture_extent,
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Uint, // offset into atlas stack; also depth or scale?
            usage: wgpu::TextureUsage::all(),
        });
        let srtm_index_texture_view =
            srtm_index_texture.create_view(&wgpu::TextureViewDescriptor {
                format: wgpu::TextureFormat::Rg16Uint,
                dimension: wgpu::TextureViewDimension::D2,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                array_layer_count: 1,
            });
        let srtm_index_texture_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: wgpu::CompareFunction::Never,
        });

        // The atlas texture is a 2d array of tiles. All tiles have the same size, but may be
        // pre-sampled at various scaling factors, allowing us to use a single atlas for all
        // resolutions. Management of tile layers is done on the CPU between frames, using the
        // patch tree to figure out what is going to be most useful to have in the cache.
        let srtm_atlas_texture_extent = wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth: 1, // Note: the texture array size is specified elsewhere.
        };
        let srtm_atlas_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-geo-tile-atlas-texture"),
            size: srtm_atlas_texture_extent,
            array_layer_count: gpu_detail.tile_cache_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Sint,
            usage: wgpu::TextureUsage::all(),
        });
        let srtm_atlas_texture_view =
            srtm_atlas_texture.create_view(&wgpu::TextureViewDescriptor {
                format: wgpu::TextureFormat::R16Sint, // heights
                dimension: wgpu::TextureViewDimension::D2Array,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                array_layer_count: gpu_detail.tile_cache_size,
            });
        let srtm_atlas_texture_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, // We should be able to mip between levels...
            lod_min_clamp: 0f32,
            lod_max_clamp: 9_999_999f32,
            compare: wgpu::CompareFunction::Never,
        });

        let bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain-geo-tile-bind-group-layout"),
                    bindings: &[
                        // SRTM Index
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: wgpu::TextureComponentType::Uint,
                                multisampled: false,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                        // SRTM Height Atlas
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2Array,
                                component_type: wgpu::TextureComponentType::Sint,
                                multisampled: false,
                            },
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                        },
                    ],
                });

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-geo-tile-bind-group"),
            layout: &bind_group_layout,
            bindings: &[
                // Height Index
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&srtm_index_texture_view),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&srtm_index_texture_sampler),
                },
                // Height Atlas
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&srtm_atlas_texture_view),
                },
                wgpu::Binding {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&srtm_atlas_texture_sampler),
                },
            ],
        });

        // Each dataset has an index.json describing the quadtree and where to find elements.
        let srtm_index_json = {
            let mut index_path = srtm_path.clone();
            index_path.push("index.json");
            let mut index_file = File::open(index_path.as_path())?;
            let mut index_content = String::new();
            index_file.read_to_string(&mut index_content)?;
            json::parse(&index_content)?
        };
        let tree = QuadTree::from_json(&srtm_path, &srtm_index_json)?;

        // FIXME: test that our basic primitives work as expected.
        /*
        let root_data = {
            let mut path = srtm_path;
            path.push(srtm_index_json["path"].as_str().expect("string"));
            let mut fp = File::open(&path)?;
            let mut data = [0u8; 2 * 512 * 512];
            fp.read_exact(&mut data)?;
            //data
            let as2: &[u8] = &data;
            let result_data: LayoutVerified<&[u8], [u16]> = LayoutVerified::new_slice(as2).unwrap();
            result_data.into_slice().to_owned()
        };

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terrain-geo-initial-texture-uploader-command-encoder"),
            });
        let buffer = gpu.push_slice(
            "terrain-geo-root-atlas-upload-buffer",
            &root_data,
            wgpu::BufferUsage::COPY_SRC,
        );
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &buffer,
                offset: 0,
                bytes_per_row: srtm_atlas_texture_extent.width * 2,
                rows_per_image: srtm_atlas_texture_extent.height,
            },
            wgpu::TextureCopyView {
                texture: &srtm_atlas_texture,
                mip_level: 0,
                array_layer: 0u32, // FIXME: hardcoded until we get the index working
                origin: wgpu::Origin3d::ZERO,
            },
            srtm_atlas_texture_extent,
        );
        gpu.queue_mut().submit(&[encoder.finish()]);
        gpu.device().poll(wgpu::Maintain::Wait);
         */

        Ok(Self {
            srtm_index_texture_extent,
            srtm_index_texture,
            srtm_index_texture_view,
            srtm_index_texture_sampler,

            srtm_atlas_texture_extent,
            srtm_atlas_texture,
            srtm_atlas_texture_view,
            srtm_atlas_texture_sampler,

            bind_group_layout,
            bind_group,

            tree,
        })
    }

    pub fn note_required(&mut self, grat: &Graticule<GeoCenter>) {
        self.tree.note_required(grat)
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GpuDetailLevel;
    use input::InputSystem;

    #[test]
    fn test_tile_manager() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let _tm = TileManager::new(&mut gpu, &GpuDetailLevel::Low.parameters())?;
        gpu.device().poll(wgpu::Maintain::Wait);
        Ok(())
    }
}
