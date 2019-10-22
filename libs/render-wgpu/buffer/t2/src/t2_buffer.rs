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
use asset::AssetManager;
use failure::Fallible;
use gpu::GPU;
use image::{ImageBuffer, Rgba};
use lay::Layer;
use lib::Library;
use log::trace;
use mm::{MissionMap, TLoc};
use nalgebra::Matrix4;
use pal::Palette;
use pic::Pic;
use std::{collections::HashMap, sync::Arc};
use t2::Terrain;
use wgpu;

#[derive(Copy, Clone, Default)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    tex_coord: [f32; 2],
}

pub struct T2Buffer {
    mm: MissionMap,
    terrain: Arc<Box<Terrain>>,
    layer: Arc<Box<Layer>>,
    pic_data: HashMap<TLoc, Vec<u8>>,
    base_palette: Palette,
    pub used_palette: Palette,

    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    atlas_texture: wgpu::Texture,
    atlas_texture_view: wgpu::TextureView,
}

impl T2Buffer {
    pub fn new(
        mm: MissionMap,
        assets: &Arc<Box<AssetManager>>,
        lib: &Arc<Box<Library>>,
        gpu: &mut GPU,
    ) -> Fallible<Self> {
        trace!("T2Renderer::new");

        let terrain_name = mm.find_t2_for_map(&|s| lib.file_exists(s))?;
        let terrain = assets.load_t2(&terrain_name)?;

        // The following are used in FA:
        //    cloud1b.LAY 1
        //    day2b.LAY 0
        //    day2b.LAY 4
        //    day2e.LAY 0
        //    day2f.LAY 0
        //    day2.LAY 0
        //    day2v.LAY 0
        let layer = assets.load_lay(&mm.layer_name.to_uppercase())?;

        let mut pic_data = HashMap::new();
        let texture_base_name = mm.get_base_texture_name()?;
        for tmap in mm.tmaps.values() {
            if !pic_data.contains_key(&tmap.loc) {
                let name = tmap.loc.pic_file(&texture_base_name);
                let data = lib.load(&name)?.to_vec();
                pic_data.insert(tmap.loc.clone(), data);
            }
        }

        let base_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

        let atlas_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {},
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsage::all(),
        });
        let atlas_texture_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            format: wgpu::TextureFormat::Rgba8Unorm,
            dimension: wgpu::TextureViewDimension::D2,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: 1, // mip level
            base_array_layer: 0,
            array_layer_count: 1,
        });
        let sampler_resource = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
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
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler_resource),
                },
            ],
        });

        let bind_group_layout = gpu
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { bindings: &[] });
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[],
        });

        let mut t2_buffer = Self {
            mm,
            terrain,
            layer,
            pic_data,
            base_palette: base_palette.clone(),
            used_palette: base_palette,
            bind_group_layout,
            bind_group,
            vertex_buffer: gpu.device().create_buffer(1, wgpu::BufferUsage::all()),
            index_buffer: gpu.device().create_buffer(1, wgpu::BufferUsage::all()),
            atlas_texture,
            atlas_texture_view,
        };
        let (bind_group, vertex_buffer, index_buffer, palette) =
            t2_renderer.regenerate_with_palette_parameters(window, 0, 0, 0, 0, 0)?;

        t2_renderer.bind_group_layout = bind_group_layout;
        t2_renderer.bind_group = bind_group;
        t2_renderer.vertex_buffer = vertex_buffer;
        t2_renderer.index_buffer = index_buffer;
        t2_renderer.used_palette = palette;

        Ok(t2_renderer)
    }

    pub fn set_palette_parameters(
        &mut self,
        window: &GraphicsWindow,
        lay_base: i32,
        e0_off: i32,
        f1_off: i32,
        c2_off: i32,
        d3_off: i32,
    ) -> Fallible<()> {
        let (vertex_buffer, index_buffer, palette) = self
            .regenerate_with_palette_parameters(window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
        self.vertex_buffer = vertex_buffer;
        self.index_buffer = index_buffer;
        self.used_palette = palette;
        Ok(())
    }

    fn regenerate_with_palette_parameters(
        &self,
        device: &wgpu::Device,
        lay_base: i32,
        e0_off: i32,
        f1_off: i32,
        c2_off: i32,
        d3_off: i32,
    ) -> Fallible<(wgpu::Buffer, wgpu::Buffer, Palette)> {
        // Note: we need to really find the right palette.
        let mut palette = self.base_palette.clone();
        let layer_data = self.layer.for_index(self.mm.layer_index + 2, lay_base)?;
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
        // FIXME: this is close except on TVIET, which needs some fiddling around 0xC0.
        palette.overlay_at(&r2, (0xC0 + c2_off) as usize)?;
        palette.overlay_at(&r3, (0xD0 + d3_off) as usize)?;
        palette.overlay_at(&r0, (0xE0 + e0_off) as usize)?;
        palette.overlay_at(&r1, (0xF0 + f1_off) as usize)?;

        //palette.dump_png("terrain_palette")?;

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

        // Load all images with our new palette.
        let mut pics = Vec::new();
        for (tloc, data) in &self.pic_data {
            let pic = Pic::decode(&palette, data)?;
            pics.push((tloc.clone(), pic));
        }

        let atlas = TextureAtlas::new(pics)?;
        let image_buf = atlas.img.to_rgba();
        let image_dim = image_buf.dimensions();
        let extent = wgpu::Extent3d {
            width: image_dim.width,
            height: image_dim.height,
            depth: 1,
        };
        let image_data = image_buf.into_raw();
        let transfer_buffer = device
            .create_buffer_mapped(img.len(), wgpu::BufferUsage::all())
            .fill_from_slice(image_data);
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                transfer_buffer,
                offset: 0,
                row_pitch: extent.width * 4,
                image_height: extent.height,
            },
            wgpu::TextureCopyView {
                texture,
                mip_level: 0,
                array_layer: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            extent,
        );

        /*
        let (texture, tex_future) = Self::upload_texture_rgba(window, atlas.img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;

        let (vertex_buffer, index_buffer) =
            self.upload_terrain_textured_simple(&atlas, &palette, window)?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(self.pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );
        */

        Ok((pds, vertex_buffer, index_buffer, palette))
    }

    fn sample_at(&self, palette: &Palette, xi: u32, zi: u32) -> ([f32; 3], [f32; 4]) {
        let offset = (zi * self.terrain.width + xi) as usize;
        let sample = if offset < self.terrain.samples.len() {
            self.terrain.samples[offset]
        } else {
            let offset = ((zi - 1) * self.terrain.width + xi) as usize;
            if offset < self.terrain.samples.len() {
                self.terrain.samples[offset]
            } else {
                let offset = ((zi - 1) * self.terrain.width + (xi - 1)) as usize;
                self.terrain.samples[offset]
            }
        };

        let x = xi as f32 / (self.terrain.width as f32) - 0.5;
        let z = zi as f32 / (self.terrain.height as f32) - 0.5;
        let h = -f32::from(sample.height) / 512f32;

        let mut color = palette.rgba(sample.color as usize).unwrap();
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
        &self,
        atlas: &TextureAtlas,
        palette: &Palette,
        device: &wgpu::Device,
    ) -> Fallible<(wgpu::Buffer, wgpu::Buffer)> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        for zi_base in (0..self.terrain.height).step_by(4) {
            for xi_base in (0..self.terrain.width).step_by(4) {
                let base = verts.len() as u32;

                // Upload all vertices in patch.
                if let Some(tmap) = self.mm.tmaps.get(&(xi_base, zi_base)) {
                    let frame = &atlas.frames[&tmap.loc];

                    for z_off in 0..5 {
                        for x_off in 0..5 {
                            let zi = zi_base + z_off;
                            let xi = xi_base + x_off;
                            let (position, _samp_color) = self.sample_at(palette, xi, zi);

                            verts.push(Vertex {
                                position,
                                color: [0f32, 0f32, 0f32, 0f32],
                                tex_coord: frame.interp(
                                    x_off as f32 / 4f32,
                                    z_off as f32 / 4f32,
                                    &tmap.orientation,
                                )?,
                            });
                        }
                    }
                } else {
                    for z_off in 0..5 {
                        for x_off in 0..5 {
                            let zi = zi_base + z_off;
                            let xi = xi_base + x_off;
                            let (position, color) = self.sample_at(palette, xi, zi);

                            verts.push(Vertex {
                                position,
                                color,
                                tex_coord: [0f32, 0f32],
                            });
                        }
                    }
                }

                // There is a fixed strip pattern here that we could probably make use of.
                // For now just re-compute per patch with the base offset.
                for row in 0..4 {
                    let row_off = row * 5;

                    indices.push(base + row_off);
                    indices.push(base + row_off);

                    for column in 0..5 {
                        indices.push(base + row_off + column);
                        indices.push(base + row_off + column + 5);
                    }

                    indices.push(base + row_off + 4 + 5);
                    indices.push(base + row_off + 4 + 5);
                }
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

        Ok((vertex_buffer, index_buffer))
    }

    /*
    fn upload_texture_rgba(
        device: &wgpu::Device,
        image_buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
    ) -> Fallible<wgpu::Texture> {
        let image_dim = image_buf.dimensions();
        let image_data = image_buf.into_raw().clone();

        let dimensions = Dimensions::Dim2d {
            width: image_dim.0,
            height: image_dim.1,
        };

        let (texture, tex_future) = ImmutableImage::from_iter(
            image_data.iter().cloned(),
            dimensions,
            Format::R8G8B8A8Unorm,
            window.queue(),
        )?;
        Ok((texture, Box::new(tex_future) as Box<dyn GpuFuture>))
    }

    fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
        let sampler = Sampler::new(
            device.clone(),
            Filter::Nearest,
            Filter::Nearest,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        )?;

        Ok(sampler)
    }

    pub fn before_frame(&mut self, camera: &dyn CameraAbstract) -> Fallible<()> {
        self.push_constants
            .set_projection(camera.projection_matrix() * camera.view_matrix());
        Ok(())
    }

    pub fn render(
        &self,
        command_buffer: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        Ok(command_buffer.draw_indexed(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.vertex_buffer.clone().unwrap()],
            self.index_buffer.clone().unwrap(),
            self.pds.clone().unwrap(),
            self.push_constants,
        )?)
    }
    */
}
