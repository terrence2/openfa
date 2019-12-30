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
use failure::{ensure, Fallible};
use fnt::Fnt;
use frame_graph::CopyBufferDescriptor;
use glyph_cache::GlyphCache;
use gpu::GPU;
use lib::Library;
use log::trace;
use memoffset::offset_of;
use std::{cell::RefCell, collections::HashMap, mem, ops::Range, rc::Rc, sync::Arc};
use zerocopy::{AsBytes, FromBytes};

// Fallback for when we have no libs loaded.
// https://fonts.google.com/specimen/Quantico?selection.family=Quantico
const QUANTICO_TTF_DATA: &[u8] = include_bytes!("../../../../../assets/font/quantico.ttf");

const SPACE_WIDTH: f32 = 5f32 / 640f32;

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug, Default)]
pub struct LayoutVertex {
    position: [f32; 2],
    tex_coord: [f32; 2],
}

impl LayoutVertex {
    #[allow(clippy::unneeded_field_pattern)]
    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        let tmp = wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float2,
                    offset: 0,
                    shader_location: 0,
                },
                // tex_coord
                wgpu::VertexAttributeDescriptor {
                    format: wgpu::VertexFormat::Float2,
                    offset: 8,
                    shader_location: 1,
                },
            ],
        };

        assert_eq!(
            tmp.attributes[0].offset,
            offset_of!(LayoutVertex, position) as wgpu::BufferAddress
        );
        assert_eq!(
            tmp.attributes[1].offset,
            offset_of!(LayoutVertex, tex_coord) as wgpu::BufferAddress
        );

        tmp
    }
}

#[derive(Copy, Clone, Debug)]
pub enum TextAnchorH {
    Center,
    Left,
    Right,
}

#[derive(Copy, Clone, Debug)]
pub enum TextAnchorV {
    Center,
    Top,
    Bottom,
    // TODO: look for empty space under '1' or 'a' or similar.
    // Baseline,
}

#[derive(Copy, Clone, Debug)]
pub enum TextPositionH {
    // In vulkan screen space: -1.0 -> 1.0
    Vulkan(f32),

    // In FA screen space: 0 -> 640
    FA(u32),

    // Labeled positions
    Center,
    Left,
    Right,
}

impl TextPositionH {
    fn to_vulkan(self) -> f32 {
        const SCALE: f32 = 640f32;
        match self {
            TextPositionH::Center => 0f32,
            TextPositionH::Left => -1f32,
            TextPositionH::Right => 1f32,
            TextPositionH::Vulkan(v) => v,
            TextPositionH::FA(i) => (i as f32) / SCALE * 2f32 - 1f32,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum TextPositionV {
    // In vulkan screen space: -1.0 -> 1.0
    Vulkan(f32),

    // In FA screen space: 0 -> 640 or 0 -> 480 depending on axis
    FA(u32),

    // Labeled positions
    Center,
    Top,
    Bottom,
}

impl TextPositionV {
    fn to_vulkan(self) -> f32 {
        const SCALE: f32 = 480f32;
        match self {
            TextPositionV::Center => 0f32,
            TextPositionV::Top => -1f32,
            TextPositionV::Bottom => 1f32,
            TextPositionV::Vulkan(v) => v,
            TextPositionV::FA(i) => (i as f32) / SCALE * 2f32 - 1f32,
        }
    }
}

#[derive(Clone)]
pub struct LayoutHandle {
    layout: Rc<RefCell<Layout>>,
}

impl LayoutHandle {
    pub fn new(layout: Layout) -> Self {
        Self {
            layout: Rc::new(RefCell::new(layout)),
        }
    }

    pub fn with_span(self, span: &str, device: &wgpu::Device) -> Fallible<Self> {
        self.set_span(span, device)?;
        Ok(self)
    }

    pub fn with_color(self, clr: &[f32; 4]) -> Self {
        self.set_color(clr);
        self
    }

    pub fn with_horizontal_position(self, pos: TextPositionH) -> Self {
        self.set_horizontal_position(pos);
        self
    }

    pub fn with_vertical_position(self, pos: TextPositionV) -> Self {
        self.set_vertical_position(pos);
        self
    }

    pub fn with_horizontal_anchor(self, anchor: TextAnchorH) -> Self {
        self.set_horizontal_anchor(anchor);
        self
    }

    pub fn with_vertical_anchor(self, anchor: TextAnchorV) -> Self {
        self.set_vertical_anchor(anchor);
        self
    }

    pub fn set_horizontal_position(&self, pos: TextPositionH) {
        self.layout.borrow_mut().position_x = pos;
    }

    pub fn set_vertical_position(&self, pos: TextPositionV) {
        self.layout.borrow_mut().position_y = pos;
    }

    pub fn set_horizontal_anchor(&self, anchor: TextAnchorH) {
        self.layout.borrow_mut().anchor_x = anchor;
    }

    pub fn set_vertical_anchor(&self, anchor: TextAnchorV) {
        self.layout.borrow_mut().anchor_y = anchor;
    }

    pub fn set_color(&self, color: &[f32; 4]) {
        self.layout.borrow_mut().color = *color;
    }

    pub fn set_span(&self, span: &str, device: &wgpu::Device) -> Fallible<()> {
        self.layout.borrow_mut().set_span(span, device)?;
        Ok(())
    }

    pub fn make_upload_buffer(
        &self,
        device: &wgpu::Device,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) {
        let layout = self.layout.borrow();

        let x = layout.position_x.to_vulkan();
        let y = layout.position_y.to_vulkan();

        let dx = match layout.anchor_x {
            TextAnchorH::Left => 0f32,
            TextAnchorH::Right => -layout.render_width,
            TextAnchorH::Center => -layout.render_width / 2f32,
        };

        let dy = match layout.anchor_y {
            TextAnchorV::Top => 0f32,
            TextAnchorV::Bottom => -layout.glyph_cache.render_height(),
            TextAnchorV::Center => -layout.glyph_cache.render_height() / 2f32,
        };

        let buffer = device
            .create_buffer_mapped(1, wgpu::BufferUsage::all())
            .fill_from_slice(&[LayoutData {
                text_layout_position: [x + dx, y + dy, 0f32, 0f32],
                text_layout_color: layout.color,
            }]);
        upload_buffers.push(CopyBufferDescriptor::new(
            buffer,
            layout.layout_data_buffer.clone(),
            mem::size_of::<LayoutData>() as wgpu::BufferAddress,
        ));
    }

    pub fn vertex_buffer(&self) -> Arc<Box<wgpu::Buffer>> {
        self.layout.borrow().vertex_buffer.clone()
    }

    pub fn index_buffer(&self) -> Arc<Box<wgpu::Buffer>> {
        self.layout.borrow().index_buffer.clone()
    }

    pub fn index_range(&self) -> Range<u32> {
        0u32..self.layout.borrow().index_count
    }

    pub fn bind_group(&self) -> Arc<Box<wgpu::BindGroup>> {
        self.layout.borrow().bind_group.clone()
    }
}

#[repr(C)]
#[derive(AsBytes, FromBytes, Copy, Clone, Debug)]
struct LayoutData {
    text_layout_position: [f32; 4],
    text_layout_color: [f32; 4],
}

// Note that each layout has its own vertex/index buffer and a tiny transform
// buffer that might get updated every frame. This is costly per layout. However,
// these are screen text layouts, so there will hopefully never be too many of them
// if we do end up creating lots, we'll need to do some sort of layout caching.
pub struct Layout {
    // The font used for rendering this layout.
    glyph_cache: Rc<Box<GlyphCache>>,

    // Cached per-frame render state.
    render_width: f32,
    position_x: TextPositionH,
    position_y: TextPositionV,
    anchor_x: TextAnchorH,
    anchor_y: TextAnchorV,
    color: [f32; 4],

    // Gpu resources
    vertex_buffer: Arc<Box<wgpu::Buffer>>,
    index_buffer: Arc<Box<wgpu::Buffer>>,
    index_count: u32,
    layout_data_buffer: Arc<Box<wgpu::Buffer>>,
    bind_group: Arc<Box<wgpu::BindGroup>>,
}

impl Layout {
    fn new(
        text: &str,
        glyph_cache: Rc<Box<GlyphCache>>,
        bind_group_layout: &wgpu::BindGroupLayout,
        device: &wgpu::Device,
    ) -> Fallible<Self> {
        let size = mem::size_of::<LayoutData>() as wgpu::BufferAddress;
        let layout_data_buffer =
            Arc::new(Box::new(device.create_buffer(&wgpu::BufferDescriptor {
                size,
                usage: wgpu::BufferUsage::all(),
            })));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &layout_data_buffer,
                    range: 0..size,
                },
            }],
        });

        let (render_width, vb, ib, index_count) =
            Self::build_text_span(text, &glyph_cache, device)?;
        Ok(Self {
            glyph_cache,

            render_width,
            position_x: TextPositionH::Center,
            position_y: TextPositionV::Center,
            anchor_x: TextAnchorH::Left,
            anchor_y: TextAnchorV::Top,
            color: [1f32, 0f32, 1f32, 1f32],

            vertex_buffer: Arc::new(Box::new(vb)),
            index_buffer: Arc::new(Box::new(ib)),
            index_count,
            layout_data_buffer,
            bind_group: Arc::new(Box::new(bind_group)),
        })
    }

    fn set_span(&mut self, text: &str, device: &wgpu::Device) -> Fallible<()> {
        let (render_width, vb, ib, index_count) =
            Self::build_text_span(text, &self.glyph_cache, device)?;
        self.render_width = render_width;
        self.vertex_buffer = Arc::new(Box::new(vb));
        self.index_buffer = Arc::new(Box::new(ib));
        self.index_count = index_count;
        Ok(())
    }

    fn build_text_span(
        text: &str,
        glyph_cache: &GlyphCache,
        device: &wgpu::Device,
    ) -> Fallible<(f32, wgpu::Buffer, wgpu::Buffer, u32)> {
        let mut verts = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let mut offset = 0f32;
        let mut prior = None;
        for mut c in text.chars() {
            if c == ' ' {
                offset += SPACE_WIDTH;
                continue;
            }

            if !glyph_cache.can_render_char(c) {
                c = '?';
            }

            let frame = glyph_cache.frame_for(c);
            let kerning = if let Some(p) = prior {
                glyph_cache.pair_kerning(p, c)
            } else {
                0f32
            };
            prior = Some(c);

            // Always do layout from 0-> and let our transform put us in the right spot.
            let x0 = offset + frame.left_side_bearing + kerning;
            let x1 = offset + frame.advance_width;
            let y0 = 0f32;
            let y1 = glyph_cache.render_height();

            let base = verts.len() as u32;
            verts.push(LayoutVertex {
                position: [x0, y0],
                tex_coord: [frame.s0, 0f32],
            });
            verts.push(LayoutVertex {
                position: [x0, y1],
                tex_coord: [frame.s0, 1f32],
            });
            verts.push(LayoutVertex {
                position: [x1, y0],
                tex_coord: [frame.s1, 0f32],
            });
            verts.push(LayoutVertex {
                position: [x1, y1],
                tex_coord: [frame.s1, 1f32],
            });

            indices.push(base);
            indices.push(base + 1u32);
            indices.push(base + 3u32);
            indices.push(base);
            indices.push(base + 3u32);
            indices.push(base + 2u32);

            offset += frame.advance_width;
        }

        let vertex_buffer = device
            .create_buffer_mapped(verts.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&verts);
        let index_buffer = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::all())
            .fill_from_slice(&indices);

        Ok((offset, vertex_buffer, index_buffer, indices.len() as u32))
    }
}

const _FONT_FILES: [&str; 12] = [
    "4X12.FNT",
    "4X6.FNT",
    "HUD00.FNT",
    "HUD01.FNT",
    "HUD11.FNT",
    "HUDSYM00.FNT",
    "HUDSYM01.FNT",
    "HUDSYM11.FNT",
    "MAPFONT.FNT",
    "WIN00.FNT",
    "WIN01.FNT",
    "WIN11.FNT",
];

const _FONT_BACKGROUNDS: [&str; 21] = [
    "ARMFONT.PIC",
    "BODYFONT.PIC",
    "BOLDFONT.PIC",
    "FNTWPNB.PIC",
    "FNTWPNY.PIC",
    "FONT4X6.PIC",
    "FONTACD.PIC",
    "FONTACT.PIC",
    "FONTDFD.PIC",
    "FONTDFT.PIC",
    "HEADFONT.PIC",
    "LRGFONT.PIC",
    "MAPFONT.PIC",
    "MENUFONT.PIC",
    "MFONT320.PIC",
    "MPFONT.PIC",
    "PANELFNT.PIC",
    "PANLFNT2.PIC",
    "SMLFONT.PIC",
    "VIDEOFNT.PIC",
    "WHEELFNT.PIC",
];

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Font {
    HUD11,
    QUANTICO,
}

pub struct LayoutBuffer {
    glyph_caches: HashMap<Font, Rc<Box<GlyphCache>>>,
    layouts: HashMap<Font, Vec<LayoutHandle>>,
    glyph_bind_group_layout: wgpu::BindGroupLayout,
    layout_bind_group_layout: wgpu::BindGroupLayout,
}

impl LayoutBuffer {
    pub fn new(lib: &Library, gpu: &mut GPU) -> Fallible<Arc<RefCell<Self>>> {
        trace!("LayoutBuffer::new");

        let glyph_bind_group_layout = GlyphCache::create_bind_group_layout(gpu.device());
        let mut glyph_caches = HashMap::new();

        // Cache all standard fonts on the GPU.
        for (name, filename) in &[(Font::HUD11, "HUD11.FNT")] {
            if let Ok(data) = lib.load(filename) {
                glyph_caches.insert(
                    *name,
                    Rc::new(Box::new(GlyphCache::new_transparent_fnt(
                        &Fnt::from_bytes("", "", &data)?,
                        &glyph_bind_group_layout,
                        gpu,
                    )?)),
                );
            }
        }

        // Add fallback font.
        glyph_caches.insert(
            Font::QUANTICO,
            Rc::new(Box::new(GlyphCache::new_ttf(
                &QUANTICO_TTF_DATA,
                &glyph_bind_group_layout,
                gpu,
            )?)),
        );

        let layout_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    bindings: &[wgpu::BindGroupLayoutBinding {
                        binding: 0,
                        visibility: wgpu::ShaderStage::VERTEX,
                        ty: wgpu::BindingType::StorageBuffer {
                            dynamic: false,
                            readonly: true,
                        },
                    }],
                });

        Ok(Arc::new(RefCell::new(Self {
            glyph_caches,
            layouts: HashMap::new(),
            glyph_bind_group_layout,
            layout_bind_group_layout,
        })))
    }

    pub fn glyph_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.glyph_bind_group_layout
    }

    pub fn layout_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout_bind_group_layout
    }

    pub fn layouts(&self) -> &HashMap<Font, Vec<LayoutHandle>> {
        &self.layouts
    }

    pub fn glyph_cache(&self, font: Font) -> Rc<Box<GlyphCache>> {
        self.glyph_caches[&font].clone()
    }

    pub fn add_screen_text(
        &mut self,
        font: Font,
        text: &str,
        device: &wgpu::Device,
    ) -> Fallible<LayoutHandle> {
        let glyph_cache = self.glyph_caches[&font].clone();
        let layout = LayoutHandle::new(Layout::new(
            text,
            glyph_cache,
            &self.layout_bind_group_layout,
            device,
        )?);
        self.layouts
            .entry(font)
            .and_modify(|e| e.push(layout.clone()))
            .or_insert_with(|| vec![layout.clone()]);
        Ok(layout)
    }

    pub fn make_upload_buffer(
        &self,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        for layouts in self.layouts.values() {
            for layout in layouts.iter() {
                layout.make_upload_buffer(gpu.device(), upload_buffers);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use input::InputSystem;
    use omnilib::OmniLib;

    #[test]
    fn it_can_render_text() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;

        let omni = OmniLib::new_for_test_in_games(&[
            "USNF", "MF", "ATF", "ATFNATO", "ATFGOLD", "USNF97", "FA",
        ])?;
        for (game, lib) in omni.libraries() {
            println!("At: {}", game);

            let layout_buffer = LayoutBuffer::new(&lib, &mut gpu)?;

            println!("1");
            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Top Left (r)", gpu.device())?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);
            println!("2");

            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Top Right (b)", gpu.device())?
                .with_color(&[0f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);

            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Bottom Left (w)", gpu.device())?
                .with_color(&[1f32, 1f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Bottom Right (m)", gpu.device())?
                .with_color(&[1f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_clr = layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "", gpu.device())?
                .with_span("THR: AFT  1.0G   2462   LCOS   740 M61", gpu.device())?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_fin = layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "DONE: 0%", gpu.device())?
                .with_color(&[0f32, 1f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Center)
                .with_vertical_anchor(TextAnchorV::Center);

            for i in 0..32 {
                if i < 16 {
                    handle_clr.set_color(&[0f32, i as f32 / 16f32, 0f32, 1f32])
                } else {
                    handle_clr.set_color(&[
                        (i as f32 - 16f32) / 16f32,
                        1f32,
                        (i as f32 - 16f32) / 16f32,
                        1f32,
                    ])
                };
                let msg = format!("DONE: {}%", ((i as f32 / 32f32) * 100f32) as u32);
                handle_fin.set_span(&msg, gpu.device())?;
            }
        }
        Ok(())
    }

    #[test]
    fn it_can_render_without_a_library() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;

        let lib = Arc::new(Box::new(Library::empty()?));
        let layout_buffer = LayoutBuffer::new(&lib, &mut gpu)?;

        layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "Top Left (r)", gpu.device())?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "Top Right (b)", gpu.device())?
            .with_color(&[0f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "Bottom Left (w)", gpu.device())?
            .with_color(&[1f32, 1f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "Bottom Right (m)", gpu.device())?
            .with_color(&[1f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_clr = layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "", gpu.device())?
            .with_span("THR: AFT  1.0G   2462   LCOS   740 M61", gpu.device())?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_fin = layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "DONE: 0%", gpu.device())?
            .with_color(&[0f32, 1f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Center)
            .with_vertical_anchor(TextAnchorV::Center);

        for i in 0..32 {
            if i < 16 {
                handle_clr.set_color(&[0f32, i as f32 / 16f32, 0f32, 1f32])
            } else {
                handle_clr.set_color(&[
                    (i as f32 - 16f32) / 16f32,
                    1f32,
                    (i as f32 - 16f32) / 16f32,
                    1f32,
                ])
            };
            let msg = format!("DONE: {}%", ((i as f32 / 32f32) * 100f32) as u32);
            handle_fin.set_span(&msg, gpu.device())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
