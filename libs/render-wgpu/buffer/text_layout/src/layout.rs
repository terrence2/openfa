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
use crate::{
    layout_vertex::LayoutVertex, LayoutHandle, LayoutTextRenderContext, TextAnchorH, TextAnchorV,
    TextPositionH, TextPositionV,
};
use failure::Fallible;
use frame_graph::FrameStateTracker;
use glyph_cache::{GlyphCache, GlyphCacheIndex};
use gpu::GPU;
use std::{cell::RefCell, collections::HashMap, mem, ops::Range, sync::Arc};
use zerocopy::{AsBytes, FromBytes};

// FIXME: this needs to be font dependent, but I'm not sure where to pull it from.
const SPACE_WIDTH: f32 = 5f32 / 640f32;

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
    pub(crate) fn new(
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

    pub(crate) fn make_upload_buffer(
        &mut self,
        glyph_cache: &GlyphCache,
        gpu: &GPU,
        tracker: &mut FrameStateTracker,
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
        tracker.upload(
            buffer,
            self.layout_data_buffer.clone(),
            mem::size_of::<LayoutData>(),
        );

        Ok(())
    }

    pub(crate) fn glyph_cache_index(&self) -> usize {
        self.glyph_cache_index.index()
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
