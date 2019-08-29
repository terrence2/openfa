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
mod glyph_cache;

use crate::glyph_cache::GlyphCache;
use failure::{ensure, Fallible};
use fnt::Fnt;
use lib::Library;
use log::trace;
use nalgebra::{Matrix4, Vector3};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::DescriptorSet,
    framebuffer::Subpass,
    impl_vertex,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
};
use window::GraphicsWindow;

// Fallback for when we have no libs loaded.
// https://fonts.google.com/specimen/Quantico?selection.family=Quantico
const QUANTICO_TTF_DATA: &[u8] = include_bytes!("../assets/quantico.ttf");

#[derive(Copy, Clone, Default)]
struct Vertex {
    position: [f32; 2],
    tex_coord: [f32; 2],
}
impl_vertex!(Vertex, position, tex_coord);

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec2 position;
            layout(location = 1) in vec2 tex_coord;

            layout(push_constant) uniform PushConstantData {
              mat4 projection;
              vec4 color;
            } pc;

            layout(location = 0) out vec2 v_tex_coord;
            layout(location = 1) flat out vec4 v_color;

            void main() {
                gl_Position = pc.projection * vec4(position, 0.0, 1.0);
                v_tex_coord = tex_coord;
                v_color = pc.color;
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
        src: "
            #version 450

            layout(location = 0) in vec2 v_tex_coord;
            layout(location = 1) in vec4 v_color;

            layout(location = 0) out vec4 f_color;

            layout(set = 0, binding = 0) uniform sampler2D tex;

            void main() {
                f_color = vec4(v_color.xyz, texture(tex, v_tex_coord).r);
            }
            "
    }
}

impl vs::ty::PushConstantData {
    fn new(m: Matrix4<f32>, c: &[f32; 4]) -> Self {
        Self {
            projection: [
                [m[0], m[1], m[1], m[3]],
                [m[4], m[5], m[6], m[7]],
                [m[8], m[9], m[7], m[11]],
                [m[12], m[13], m[14], m[15]],
            ],
            color: *c,
        }
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

    pub fn with_span(self, span: &str, window: &GraphicsWindow) -> Fallible<Self> {
        self.set_span(span, window)?;
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

    pub fn set_projection(&self, w: f32, h: f32) {
        self.layout.borrow_mut().scale = [w, h];
    }

    pub fn set_color(&self, color: &[f32; 4]) {
        self.layout.borrow_mut().color = *color;
    }

    pub fn set_span(&self, span: &str, window: &GraphicsWindow) -> Fallible<()> {
        self.layout.borrow_mut().set_span(span, window)?;
        Ok(())
    }

    fn vertex_buffer(&self) -> Arc<CpuAccessibleBuffer<[Vertex]>> {
        self.layout.borrow().vertex_buffer.clone()
    }

    fn index_buffer(&self) -> Arc<CpuAccessibleBuffer<[u32]>> {
        self.layout.borrow().index_buffer.clone()
    }

    fn pds(&self) -> Arc<dyn DescriptorSet + Send + Sync> {
        self.layout.borrow().pds.clone()
    }

    fn push_consts(&self) -> Fallible<vs::ty::PushConstantData> {
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

        let scale = layout.scale;

        let m = Matrix4::new_translation(&Vector3::new(x + dx, y + dy, 0.0f32))
            * Matrix4::new_nonuniform_scaling(&Vector3::new(scale[0], scale[1], 1f32));

        Ok(vs::ty::PushConstantData::new(m, &layout.color))

        // pcd.set_projection(m);
        // pcd.set_color(&layout.color);
        // Ok(pcd)
    }
}

pub struct Layout {
    // The font used for rendering this layout.
    glyph_cache: Rc<Box<GlyphCache>>,

    // Cached per-frame render state.
    //push_consts: vs::ty::PushConstantData,
    render_width: f32,
    position_x: TextPositionH,
    position_y: TextPositionV,
    anchor_x: TextAnchorH,
    anchor_y: TextAnchorV,
    scale: [f32; 2],
    color: [f32; 4],

    // Gpu resources
    pds: Arc<dyn DescriptorSet + Send + Sync>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    index_buffer: Arc<CpuAccessibleBuffer<[u32]>>,
}

impl Layout {
    fn new(
        text: &str,
        glyph_cache: Rc<Box<GlyphCache>>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        let (render_width, pds, vb, ib) = Self::build_text_span(text, &glyph_cache, window)?;
        Ok(Self {
            glyph_cache,

            //push_consts: vs::ty::PushConstantData::new(),
            render_width,
            position_x: TextPositionH::Center,
            position_y: TextPositionV::Center,
            anchor_x: TextAnchorH::Left,
            anchor_y: TextAnchorV::Top,
            scale: [1f32, 1f32],
            color: [1f32, 0f32, 1f32, 1f32],

            pds,
            vertex_buffer: vb,
            index_buffer: ib,
        })
    }

    fn set_span(&mut self, text: &str, window: &GraphicsWindow) -> Fallible<()> {
        let (render_width, pds, vb, ib) = Self::build_text_span(text, &self.glyph_cache, window)?;
        self.render_width = render_width;
        self.pds = pds;
        self.vertex_buffer = vb;
        self.index_buffer = ib;
        Ok(())
    }

    fn build_text_span(
        text: &str,
        glyph_cache: &GlyphCache,
        window: &GraphicsWindow,
    ) -> Fallible<(
        f32,
        Arc<dyn DescriptorSet + Send + Sync>,
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
    )> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        let mut offset = 0f32;
        let mut prior = None;
        for c in text.chars() {
            if c == ' ' {
                offset += 5f32 / 640f32;
                continue;
            }

            ensure!(
                glyph_cache.can_render_char(c),
                "attempted to render nonprintable char: {}",
                c
            );

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
            verts.push(Vertex {
                position: [x0, y0],
                tex_coord: [frame.s0, 0f32],
            });
            verts.push(Vertex {
                position: [x0, y1],
                tex_coord: [frame.s0, 1f32],
            });
            verts.push(Vertex {
                position: [x1, y0],
                tex_coord: [frame.s1, 0f32],
            });
            verts.push(Vertex {
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

        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        Ok((
            offset,
            glyph_cache.descriptor_set(),
            vertex_buffer,
            index_buffer,
        ))
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

pub struct TextRenderer {
    screen_pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    glyph_caches: HashMap<Font, Rc<Box<GlyphCache>>>,
    layouts: Vec<LayoutHandle>,
}

impl TextRenderer {
    pub fn new(lib: &Arc<Box<Library>>, window: &GraphicsWindow) -> Fallible<Self> {
        trace!("TextRenderer::new");

        let vs = vs::Shader::load(window.device())?;
        let fs = fs::Shader::load(window.device())?;

        let screen_pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(vs.main_entry_point(), ())
                .triangle_list()
                .cull_mode_back()
                .front_face_counter_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
                .blend_alpha_blending()
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        );

        let mut glyph_caches = HashMap::new();

        // Cache all standard fonts on the GPU.
        for (name, filename) in &[(Font::HUD11, "HUD11.FNT")] {
            let maybe_data = lib.load(filename);
            if maybe_data.is_ok() {
                glyph_caches.insert(
                    *name,
                    Rc::new(Box::new(GlyphCache::new_transparent_fnt(
                        &Fnt::from_bytes("", "", &maybe_data.unwrap())?,
                        screen_pipeline.clone(),
                        window,
                    )?)),
                );
            }
        }

        // Add fallback font.
        glyph_caches.insert(
            Font::QUANTICO,
            Rc::new(Box::new(GlyphCache::new_ttf(
                &QUANTICO_TTF_DATA,
                screen_pipeline.clone(),
                window,
            )?)),
        );

        Ok(Self {
            glyph_caches,
            screen_pipeline,
            layouts: Vec::new(),
        })
    }

    pub fn add_screen_text(
        &mut self,
        font: Font,
        text: &str,
        window: &GraphicsWindow,
    ) -> Fallible<LayoutHandle> {
        let glyph_cache = self.glyph_caches[&font].clone();
        let layout = LayoutHandle::new(Layout::new(text, glyph_cache, window)?);
        self.layouts.push(layout.clone());
        Ok(layout)
    }

    pub fn set_projection(&mut self, window: &GraphicsWindow) -> Fallible<()> {
        for layout in &mut self.layouts {
            let dim = window.dimensions()?;
            let aspect = window.aspect_ratio()? * 4f32 / 3f32;

            let (w, h) = if dim[0] > dim[1] {
                (aspect, 1f32)
            } else {
                (1f32, 1f32 / aspect)
            };
            layout.set_projection(w, h);
        }
        Ok(())
    }

    pub fn before_frame(&mut self, window: &GraphicsWindow) -> Fallible<()> {
        self.set_projection(&window)
    }

    pub fn render(
        &self,
        cb: AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
    ) -> Fallible<AutoCommandBufferBuilder> {
        let mut cb = cb;
        for layout in &self.layouts {
            cb = cb.draw_indexed(
                self.screen_pipeline.clone(),
                dynamic_state,
                vec![layout.vertex_buffer()],
                layout.index_buffer(),
                layout.pds(),
                layout.push_consts()?,
            )?;
        }

        Ok(cb)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use omnilib::OmniLib;
    use window::GraphicsConfigBuilder;

    #[test]
    fn it_can_render_text() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        window.set_clear_color(&[0f32, 0f32, 0f32, 1f32]);

        let omni = OmniLib::new_for_test_in_games(&[
            "USNF", "MF", "ATF", "ATFNATO", "ATFGOLD", "USNF97", "FA",
        ])?;
        for (game, lib) in omni.libraries() {
            println!("At: {}", game);

            let mut renderer = TextRenderer::new(&lib, &window)?;

            renderer
                .add_screen_text(Font::HUD11, "Top Left (r)", &window)?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);

            renderer
                .add_screen_text(Font::HUD11, "Top Right (b)", &window)?
                .with_color(&[0f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Top)
                .with_vertical_anchor(TextAnchorV::Top);

            renderer
                .add_screen_text(Font::HUD11, "Bottom Left (w)", &window)?
                .with_color(&[1f32, 1f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Left)
                .with_horizontal_anchor(TextAnchorH::Left)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            renderer
                .add_screen_text(Font::HUD11, "Bottom Right (m)", &window)?
                .with_color(&[1f32, 0f32, 1f32, 1f32])
                .with_horizontal_position(TextPositionH::Right)
                .with_horizontal_anchor(TextAnchorH::Right)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_clr = renderer
                .add_screen_text(Font::HUD11, "", &window)?
                .with_span("THR: AFT  1.0G   2462   LCOS   740 M61", &window)?
                .with_color(&[1f32, 0f32, 0f32, 1f32])
                .with_horizontal_position(TextPositionH::Center)
                .with_horizontal_anchor(TextAnchorH::Center)
                .with_vertical_position(TextPositionV::Bottom)
                .with_vertical_anchor(TextAnchorV::Bottom);

            let handle_fin = renderer
                .add_screen_text(Font::HUD11, "DONE: 0%", &window)?
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
                handle_fin.set_span(&msg, &window)?;

                {
                    let frame = window.begin_frame()?;
                    if !frame.is_valid() {
                        continue;
                    }

                    renderer.before_frame(&window)?;

                    let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                        window.device(),
                        window.queue().family(),
                    )?;

                    cbb = cbb.begin_render_pass(
                        frame.framebuffer(&window),
                        false,
                        vec![[0f32, 0f32, 0f32, 1f32].into(), 0f32.into()],
                    )?;

                    cbb = renderer.render(cbb, &window.dynamic_state)?;

                    cbb = cbb.end_render_pass()?;

                    let cb = cbb.build()?;

                    frame.submit(cb, &mut window)?;
                }
            }
        }
        std::mem::drop(window);
        Ok(())
    }

    #[test]
    fn it_can_render_without_a_library() -> Fallible<()> {
        let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
        window.set_clear_color(&[0f32, 0f32, 0f32, 1f32]);

        let lib = Arc::new(Box::new(Library::empty()?));
        let mut renderer = TextRenderer::new(&lib, &window)?;

        renderer
            .add_screen_text(Font::QUANTICO, "Top Left (r)", &window)?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        renderer
            .add_screen_text(Font::QUANTICO, "Top Right (b)", &window)?
            .with_color(&[0f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Top)
            .with_vertical_anchor(TextAnchorV::Top);

        renderer
            .add_screen_text(Font::QUANTICO, "Bottom Left (w)", &window)?
            .with_color(&[1f32, 1f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Left)
            .with_horizontal_anchor(TextAnchorH::Left)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        renderer
            .add_screen_text(Font::QUANTICO, "Bottom Right (m)", &window)?
            .with_color(&[1f32, 0f32, 1f32, 1f32])
            .with_horizontal_position(TextPositionH::Right)
            .with_horizontal_anchor(TextAnchorH::Right)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_clr = renderer
            .add_screen_text(Font::QUANTICO, "", &window)?
            .with_span("THR: AFT  1.0G   2462   LCOS   740 M61", &window)?
            .with_color(&[1f32, 0f32, 0f32, 1f32])
            .with_horizontal_position(TextPositionH::Center)
            .with_horizontal_anchor(TextAnchorH::Center)
            .with_vertical_position(TextPositionV::Bottom)
            .with_vertical_anchor(TextAnchorV::Bottom);

        let handle_fin = renderer
            .add_screen_text(Font::QUANTICO, "DONE: 0%", &window)?
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
            handle_fin.set_span(&msg, &window)?;

            {
                let frame = window.begin_frame()?;
                if !frame.is_valid() {
                    continue;
                }

                renderer.before_frame(&window)?;

                let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                    window.device(),
                    window.queue().family(),
                )?;

                cbb = cbb.begin_render_pass(
                    frame.framebuffer(&window),
                    false,
                    vec![[0f32, 0f32, 0f32, 1f32].into(), 0f32.into()],
                )?;

                cbb = renderer.render(cbb, &window.dynamic_state)?;

                cbb = cbb.end_render_pass()?;

                let cb = cbb.build()?;

                frame.submit(cb, &mut window)?;
            }
        }
        std::mem::drop(window);
        Ok(())
    }
}
