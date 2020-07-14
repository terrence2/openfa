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
mod layout;
mod layout_vertex;

use crate::layout::Layout;
pub use crate::layout_vertex::LayoutVertex;

use failure::Fallible;
use font_ttf::TtfFont;
use frame_graph::FrameStateTracker;
use glyph_cache::{GlyphCache, GlyphCacheIndex};
use gpu::GPU;
use log::trace;
use std::{cell::RefCell, collections::HashMap, mem, ops::Range, sync::Arc};
use zerocopy::{AsBytes, FromBytes};

// Fallback for when we have no libs loaded.
// https://fonts.google.com/specimen/Quantico?selection.family=Quantico
pub const FALLBACK_FONT_NAME: &'static str = "quantico";
const QUANTICO_TTF_DATA: &[u8] = include_bytes!("../../../../../assets/font/quantico.ttf");

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
    pub fn grab<'a>(&self, buffer: &'a mut TextLayoutBuffer) -> &'a mut Layout {
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

pub type FontName = String;

pub struct TextLayoutBuffer {
    glyph_cache_map: HashMap<FontName, GlyphCacheIndex>,
    glyph_caches: Vec<GlyphCache>,
    layout_map: HashMap<FontName, Vec<LayoutHandle>>,
    layouts: Vec<Layout>,
    glyph_bind_group_layout: wgpu::BindGroupLayout,
    layout_bind_group_layout: wgpu::BindGroupLayout,
}

impl TextLayoutBuffer {
    pub fn new(gpu: &mut GPU) -> Fallible<Arc<RefCell<Self>>> {
        trace!("LayoutBuffer::new");

        let glyph_bind_group_layout = GlyphCache::create_bind_group_layout(gpu.device());
        let mut glyph_caches = Vec::new();
        let mut glyph_cache_map = HashMap::new();

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

        // Add fallback font.
        let index = GlyphCacheIndex::new(glyph_caches.len());
        glyph_cache_map.insert(FALLBACK_FONT_NAME.to_owned(), index);
        glyph_caches.push(GlyphCache::new(
            index,
            TtfFont::new(&QUANTICO_TTF_DATA, &glyph_bind_group_layout, gpu)?,
        ));

        Ok(Arc::new(RefCell::new(Self {
            glyph_cache_map,
            glyph_caches,
            layout_map: HashMap::new(),
            layouts: Vec::new(),
            glyph_bind_group_layout,
            layout_bind_group_layout,
        })))
    }

    pub fn load_font(&mut self, _font_name: FontName, _glyphs: GlyphCache) {
        // Cache all standard fonts on the GPU.
        /*
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
         */
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

    pub fn layouts_by_font(&self) -> &HashMap<FontName, Vec<LayoutHandle>> {
        &self.layout_map
    }

    pub fn layout(&self, handle: LayoutHandle) -> &Layout {
        &self.layouts[handle.0]
    }

    pub fn layout_mut(&mut self, handle: LayoutHandle) -> &mut Layout {
        &mut self.layouts[handle.0]
    }

    pub fn glyph_cache(&self, font_name: &str) -> &GlyphCache {
        if let Some(id) = self.glyph_cache_map.get(font_name) {
            return &self.glyph_caches[id.index()];
        }
        &self.glyph_caches[self.glyph_cache_map[FALLBACK_FONT_NAME].index()]
    }

    // pub fn layout_mut(&mut self, handle: LayoutHandle) -> &mut Layout {}

    pub fn add_screen_text(
        &mut self,
        font_name: &str,
        text: &str,
        gpu: &GPU,
    ) -> Fallible<&mut Layout> {
        let glyph_cache = self.glyph_cache(font_name);
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
            .entry(font_name.to_owned())
            .and_modify(|e| e.push(handle))
            .or_insert_with(|| vec![handle]);
        Ok(self.layout_mut(handle))
    }

    pub fn make_upload_buffer(
        &mut self,
        gpu: &GPU,
        tracker: &mut FrameStateTracker,
    ) -> Fallible<()> {
        for layout in self.layouts.iter_mut() {
            layout.make_upload_buffer(
                &self.glyph_caches[layout.glyph_cache_index()],
                gpu,
                tracker,
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
            // FIXME: Load all fonts for the game.
            println!("At: {}", game);

            let layout_buffer = TextLayoutBuffer::new(&mut gpu)?;

            layout_buffer
                .borrow_mut()
                .add_screen_text("quantico", "Top Left (r)", &gpu)?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top)
                .handle();

            layout_buffer
                .borrow_mut()
                .add_screen_text("quantico", "Top Right (b)", &gpu)?
                .with_color(&[0f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);

            layout_buffer
                .borrow_mut()
                .add_screen_text("quantico", "Bottom Left (w)", &gpu)?
                .with_color(&[1f32, 1f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            layout_buffer
                .borrow_mut()
                .add_screen_text("quantico", "Bottom Right (m)", &gpu)?
                .with_color(&[1f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_clr = layout_buffer
                .borrow_mut()
                .add_screen_text("quantico", "", &gpu)?
                .with_span("THR: AFT  1.0G   2462   LCOS   740 M61")
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom)
                .handle();

            let handle_fin = layout_buffer
                .borrow_mut()
                .add_screen_text("quantico", "DONE: 0%", &gpu)?
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

        let layout_buffer = TextLayoutBuffer::new(&mut gpu)?;

        layout_buffer
            .borrow_mut()
            .add_screen_text("quantico", "Top Left (r)", &gpu)?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .borrow_mut()
            .add_screen_text("quantico", "Top Right (b)", &gpu)?
            .with_color(&[0f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        layout_buffer
            .borrow_mut()
            .add_screen_text("quantico", "Bottom Left (w)", &gpu)?
            .with_color(&[1f32, 1f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        layout_buffer
            .borrow_mut()
            .add_screen_text("quantico", "Bottom Right (m)", &gpu)?
            .with_color(&[1f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_clr = layout_buffer
            .borrow_mut()
            .add_screen_text("quantico", "", &gpu)?
            .with_span("THR: AFT  1.0G   2462   LCOS   740 M61")
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom)
            .handle();

        let handle_fin = layout_buffer
            .borrow_mut()
            .add_screen_text("quantico", "DONE: 0%", &gpu)?
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
