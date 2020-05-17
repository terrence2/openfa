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
use failure::Fallible;
use fnt::Fnt;
use frame_graph::CopyBufferDescriptor;
use glyph_cache::{GlyphCache, GlyphCacheIndex};
use gpu::GPU;
use lib::Library;
use log::trace;
use memoffset::offset_of;
use std::{cell::RefCell, collections::HashMap, mem, ops::Range, sync::Arc};
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LayoutHandle(usize);

impl LayoutHandle {
    pub fn grab<'a>(&self, buffer: &'a mut LayoutBuffer) -> &'a mut Layout {
        buffer.layout_mut(*self)
    }
}

// Context required for rendering a specific text span (as opposed to the layout in general).
// e.g. the vertex and index buffers.
struct LayoutTextRenderContext {
    render_width: f32,
    vertex_buffer: Arc<Box<wgpu::Buffer>>,
    index_buffer: Arc<Box<wgpu::Buffer>>,
    index_count: u32,
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
    // The externally exposed handle, for ease of use.
    layout_handle: LayoutHandle,

    // The font used for rendering this layout.
    glyph_cache_index: GlyphCacheIndex,

    // Cached per-frame render state.
    content: String,
    position_x: TextPositionH,
    position_y: TextPositionV,
    anchor_x: TextAnchorH,
    anchor_y: TextAnchorV,
    color: [f32; 4],

    // Gpu resources
    text_render_context: Option<LayoutTextRenderContext>,
    layout_data_buffer: Arc<Box<wgpu::Buffer>>,
    bind_group: Arc<Box<wgpu::BindGroup>>,
}

impl Layout {
    fn new(
        layout_handle: LayoutHandle,
        text: &str,
        glyph_cache: &GlyphCache,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &GPU,
    ) -> Fallible<Self> {
        let size = mem::size_of::<LayoutData>() as wgpu::BufferAddress;
        let layout_data_buffer = Arc::new(Box::new(gpu.device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("text-layout-data-buffer"),
                size,
                usage: wgpu::BufferUsage::all(),
            },
        )));

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text-layout-bind-group"),
            layout: &bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &layout_data_buffer,
                    range: 0..size,
                },
            }],
        });

        let text_render_context = Self::build_text_span(text, &glyph_cache, gpu)?;
        Ok(Self {
            layout_handle,
            glyph_cache_index: glyph_cache.index(),

            content: text.to_owned(),
            position_x: TextPositionH::Center,
            position_y: TextPositionV::Center,
            anchor_x: TextAnchorH::Left,
            anchor_y: TextAnchorV::Top,
            color: [1f32, 0f32, 1f32, 1f32],

            text_render_context: Some(text_render_context),
            layout_data_buffer,
            bind_group: Arc::new(Box::new(bind_group)),
        })
    }

    pub fn with_span(&mut self, span: &str) -> &mut Self {
        self.set_span(span);
        self
    }

    pub fn with_color(&mut self, clr: &[f32; 4]) -> &mut Self {
        self.set_color(clr);
        self
    }

    pub fn with_horizontal_position(&mut self, pos: TextPositionH) -> &mut Self {
        self.set_horizontal_position(pos);
        self
    }

    pub fn with_vertical_position(&mut self, pos: TextPositionV) -> &mut Self {
        self.set_vertical_position(pos);
        self
    }

    pub fn with_horizontal_anchor(&mut self, anchor: TextAnchorH) -> &mut Self {
        self.set_horizontal_anchor(anchor);
        self
    }

    pub fn with_vertical_anchor(&mut self, anchor: TextAnchorV) -> &mut Self {
        self.set_vertical_anchor(anchor);
        self
    }

    pub fn set_horizontal_position(&mut self, pos: TextPositionH) {
        self.position_x = pos;
    }

    pub fn set_vertical_position(&mut self, pos: TextPositionV) {
        self.position_y = pos;
    }

    pub fn set_horizontal_anchor(&mut self, anchor: TextAnchorH) {
        self.anchor_x = anchor;
    }

    pub fn set_vertical_anchor(&mut self, anchor: TextAnchorV) {
        self.anchor_y = anchor;
    }

    pub fn set_color(&mut self, color: &[f32; 4]) {
        self.color = *color;
    }

    pub fn set_span(&mut self, text: &str) {
        self.text_render_context = None;
        self.content = text.to_owned();
    }

    pub fn handle(&self) -> LayoutHandle {
        self.layout_handle
    }

    fn build_text_span(
        text: &str,
        glyph_cache: &GlyphCache,
        gpu: &GPU,
    ) -> Fallible<LayoutTextRenderContext> {
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

        // Create a degenerate triangle if there was nothing to render, because we cannot
        // push an empty buffer, but still want something to hold here.
        if verts.is_empty() {
            for i in 0..3 {
                verts.push(LayoutVertex {
                    position: [0f32, 0f32],
                    tex_coord: [0f32, 0f32],
                });
                indices.push(i);
            }
        }

        let vertex_buffer = gpu.push_slice(
            "text-layout-vertex-buffer",
            &verts,
            wgpu::BufferUsage::all(),
        );
        let index_buffer = gpu.push_slice(
            "text-layout-index-buffer",
            &indices,
            wgpu::BufferUsage::all(),
        );

        Ok(LayoutTextRenderContext {
            render_width: offset,
            vertex_buffer: Arc::new(Box::new(vertex_buffer)),
            index_buffer: Arc::new(Box::new(index_buffer)),
            index_count: indices.len() as u32,
        })
    }

    fn make_upload_buffer(
        &mut self,
        glyph_cache: &GlyphCache,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        if self.text_render_context.is_none() {
            self.text_render_context =
                Some(Layout::build_text_span(&self.content, &glyph_cache, gpu)?);
        }

        let x = self.position_x.to_vulkan();
        let y = self.position_y.to_vulkan();

        let dx = match self.anchor_x {
            TextAnchorH::Left => 0f32,
            TextAnchorH::Right => -self.text_render_context.as_ref().unwrap().render_width,
            TextAnchorH::Center => -self.text_render_context.as_ref().unwrap().render_width / 2f32,
        };

        let dy = match self.anchor_y {
            TextAnchorV::Top => 0f32,
            TextAnchorV::Bottom => -glyph_cache.render_height(),
            TextAnchorV::Center => -glyph_cache.render_height() / 2f32,
        };

        let buffer = gpu.push_slice(
            "text-layout-upload-buffer",
            &[LayoutData {
                text_layout_position: [x + dx, y + dy, 0f32, 0f32],
                text_layout_color: self.color,
            }],
            wgpu::BufferUsage::all(),
        );
        upload_buffers.push(CopyBufferDescriptor::new(
            buffer,
            self.layout_data_buffer.clone(),
            mem::size_of::<LayoutData>() as wgpu::BufferAddress,
        ));

        Ok(())
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.text_render_context.as_ref().unwrap().vertex_buffer
    }

    pub fn index_buffer(&self) -> &wgpu::Buffer {
        &self.text_render_context.as_ref().unwrap().index_buffer
    }

    pub fn index_range(&self) -> Range<u32> {
        0u32..self.text_render_context.as_ref().unwrap().index_count
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
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
    glyph_cache_map: HashMap<Font, GlyphCacheIndex>,
    glyph_caches: Vec<GlyphCache>,
    layout_map: HashMap<Font, Vec<LayoutHandle>>,
    layouts: Vec<Layout>,
    glyph_bind_group_layout: wgpu::BindGroupLayout,
    layout_bind_group_layout: wgpu::BindGroupLayout,
}

impl LayoutBuffer {
    pub fn new(lib: &Library, gpu: &mut GPU) -> Fallible<Arc<RefCell<Self>>> {
        trace!("LayoutBuffer::new");

        let glyph_bind_group_layout = GlyphCache::create_bind_group_layout(gpu.device());
        let mut glyph_caches = Vec::new();
        let mut glyph_cache_map = HashMap::new();

        // Cache all standard fonts on the GPU.
        for (name, filename) in &[(Font::HUD11, "HUD11.FNT")] {
            if let Ok(data) = lib.load(filename) {
                let index = GlyphCacheIndex::new(glyph_caches.len());
                glyph_cache_map.insert(*name, index);
                glyph_caches.push(GlyphCache::new_transparent_fnt(
                    &Fnt::from_bytes("", "", &data)?,
                    index,
                    &glyph_bind_group_layout,
                    gpu,
                )?);
            }
        }

        // Add fallback font.
        let index = GlyphCacheIndex::new(glyph_caches.len());
        glyph_cache_map.insert(Font::QUANTICO, index);
        glyph_caches.push(GlyphCache::new_ttf(
            &QUANTICO_TTF_DATA,
            index,
            &glyph_bind_group_layout,
            gpu,
        )?);

        let layout_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("text-layout-bind-group-layout"),
                    bindings: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::VERTEX,
                        ty: wgpu::BindingType::StorageBuffer {
                            dynamic: false,
                            readonly: true,
                        },
                    }],
                });

        Ok(Arc::new(RefCell::new(Self {
            glyph_cache_map,
            glyph_caches,
            layout_map: HashMap::new(),
            layouts: Vec::new(),
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

    pub fn layouts(&self) -> &Vec<Layout> {
        &self.layouts
    }

    pub fn layouts_by_font(&self) -> &HashMap<Font, Vec<LayoutHandle>> {
        &self.layout_map
    }

    pub fn layout(&self, handle: LayoutHandle) -> &Layout {
        &self.layouts[handle.0]
    }

    pub fn layout_mut(&mut self, handle: LayoutHandle) -> &mut Layout {
        &mut self.layouts[handle.0]
    }

    pub fn glyph_cache(&self, font: Font) -> &GlyphCache {
        &self.glyph_caches[self.glyph_cache_map[&font].index()]
    }

    // pub fn layout_mut(&mut self, handle: LayoutHandle) -> &mut Layout {}

    pub fn add_screen_text(&mut self, font: Font, text: &str, gpu: &GPU) -> Fallible<&mut Layout> {
        let glyph_cache = self.glyph_cache(font);
        let handle = LayoutHandle(self.layouts.len());
        let layout = Layout::new(
            handle,
            text,
            glyph_cache,
            &self.layout_bind_group_layout,
            gpu,
        )?;
        self.layouts.push(layout);
        self.layout_map
            .entry(font)
            .and_modify(|e| e.push(handle))
            .or_insert_with(|| vec![handle]);
        Ok(self.layout_mut(handle))
    }

    pub fn make_upload_buffer(
        &mut self,
        gpu: &GPU,
        upload_buffers: &mut Vec<CopyBufferDescriptor>,
    ) -> Fallible<()> {
        for layout in self.layouts.iter_mut() {
            layout.make_upload_buffer(
                &self.glyph_caches[layout.glyph_cache_index.index()],
                gpu,
                upload_buffers,
            )?;
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

            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Top Left (r)", &gpu)?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top)
                .handle();

            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Top Right (b)", &gpu)?
                .with_color(&[0f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);

            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Bottom Left (w)", &gpu)?
                .with_color(&[1f32, 1f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "Bottom Right (m)", &gpu)?
                .with_color(&[1f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_clr = layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "", &gpu)?
                .with_span("THR: AFT  1.0G   2462   LCOS   740 M61")
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom)
                .handle();

            let handle_fin = layout_buffer
                .borrow_mut()
                .add_screen_text(Font::HUD11, "DONE: 0%", &gpu)?
                .with_color(&[0f32, 1f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Center)
                .with_vertical_anchor(TextAnchorV::Center)
                .handle();

            for i in 0..32 {
                if i < 16 {
                    handle_clr
                        .grab(&mut layout_buffer.borrow_mut())
                        .set_color(&[0f32, i as f32 / 16f32, 0f32, 1f32]);
                } else {
                    handle_clr
                        .grab(&mut layout_buffer.borrow_mut())
                        .set_color(&[
                            (i as f32 - 16f32) / 16f32,
                            1f32,
                            (i as f32 - 16f32) / 16f32,
                            1f32,
                        ])
                };
                let msg = format!("DONE: {}%", ((i as f32 / 32f32) * 100f32) as u32);
                handle_fin
                    .grab(&mut layout_buffer.borrow_mut())
                    .set_span(&msg);
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
            .add_screen_text(Font::QUANTICO, "Top Left (r)", &gpu)?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "Top Right (b)", &gpu)?
            .with_color(&[0f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "Bottom Left (w)", &gpu)?
            .with_color(&[1f32, 1f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "Bottom Right (m)", &gpu)?
            .with_color(&[1f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_clr = layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "", &gpu)?
            .with_span("THR: AFT  1.0G   2462   LCOS   740 M61")
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom)
            .handle();

        let handle_fin = layout_buffer
            .borrow_mut()
            .add_screen_text(Font::QUANTICO, "DONE: 0%", &gpu)?
            .with_color(&[0f32, 1f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Center)
            .with_vertical_anchor(TextAnchorV::Center)
            .handle();

        for i in 0..32 {
            if i < 16 {
                handle_clr
                    .grab(&mut layout_buffer.borrow_mut())
                    .set_color(&[0f32, i as f32 / 16f32, 0f32, 1f32])
            } else {
                handle_clr
                    .grab(&mut layout_buffer.borrow_mut())
                    .set_color(&[
                        (i as f32 - 16f32) / 16f32,
                        1f32,
                        (i as f32 - 16f32) / 16f32,
                        1f32,
                    ])
            };
            let msg = format!("DONE: {}%", ((i as f32 / 32f32) * 100f32) as u32);
            handle_fin
                .grab(&mut layout_buffer.borrow_mut())
                .set_span(&msg);
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
