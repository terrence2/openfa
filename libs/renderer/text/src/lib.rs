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
use codepage_437::{ToCp437, CP437_CONTROL};
use failure::{bail, ensure, Fallible};
use fnt::Fnt;
use i386::{Interpreter, Reg};
use image::{GrayImage, ImageBuffer, Luma, Rgba};
use lib::Library;
use log::trace;
use nalgebra::{Matrix4, Vector3};
use pal::Palette;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    framebuffer::Subpass,
    image::{Dimensions, ImmutableImage},
    impl_vertex,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::GraphicsWindow;

#[derive(Copy, Clone, Debug)]
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
                f_color = vec4(v_color.xyz, texture(tex, v_tex_coord).w);
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
            TextAnchorV::Bottom => -layout.font_info.render_height,
            TextAnchorV::Center => -layout.font_info.render_height / 2f32,
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
    font_info: Rc<Box<FontInfo>>,

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
    fn new(text: &str, font_info: Rc<Box<FontInfo>>, window: &GraphicsWindow) -> Fallible<Self> {
        let (render_width, pds, vb, ib) = Self::build_text_span(text, &font_info, window)?;
        Ok(Self {
            font_info,

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
        let (render_width, pds, vb, ib) = Self::build_text_span(text, &self.font_info, window)?;
        self.render_width = render_width;
        self.pds = pds;
        self.vertex_buffer = vb;
        self.index_buffer = ib;
        Ok(())
    }

    fn build_text_span(
        text: &str,
        info: &FontInfo,
        window: &GraphicsWindow,
    ) -> Fallible<(
        f32,
        Arc<dyn DescriptorSet + Send + Sync>,
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
    )> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        let encoded = match text.to_cp437(&CP437_CONTROL) {
            Ok(encoded) => encoded,
            Err(_) => bail!("attempted to render non cp437 text"),
        };
        let mut offset = 0f32;
        for c in encoded.iter() {
            if *c == b' ' {
                offset += 5f32 / 640f32;
                continue;
            }

            ensure!(
                info.glyph_layout.contains_key(c),
                "attempted to render nonprintable char: {}",
                c
            );

            let layout = &info.glyph_layout[c];

            // Always do layout from 0-> and let our transform put us in the right spot.
            let x0 = offset;
            let x1 = offset + layout.render_width;
            let y0 = 0f32;
            let y1 = info.render_height;

            let base = verts.len() as u32;
            verts.push(Vertex {
                position: [x0, y0],
                tex_coord: [layout.s0, 0f32],
            });
            verts.push(Vertex {
                position: [x0, y1],
                tex_coord: [layout.s0, 1f32],
            });
            verts.push(Vertex {
                position: [x1, y0],
                tex_coord: [layout.s1, 0f32],
            });
            verts.push(Vertex {
                position: [x1, y1],
                tex_coord: [layout.s1, 1f32],
            });

            indices.push(base);
            indices.push(base + 1u32);
            indices.push(base + 3u32);
            indices.push(base);
            indices.push(base + 3u32);
            indices.push(base + 2u32);

            offset += layout.render_width;
        }

        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        Ok((offset, info.pds.clone(), vertex_buffer, index_buffer))
    }
}

struct GlyphLayout {
    // Left and right texture coordinates.
    s0: f32,
    s1: f32,

    // Width scaled into the right perspective for rendering.
    render_width: f32,
}

struct FontInfo {
    // These get composited in software, then uploaded in a single texture.
    pds: Arc<dyn DescriptorSet + Send + Sync>,

    // The positions of glyphs within the texture (as needed for layout later)
    // are stored in a map by glyph index.
    glyph_layout: HashMap<u8, GlyphLayout>,

    // The intended render height of the font in vulkan coordinates.
    render_height: f32,
}

impl FontInfo {
    fn new_transparent(
        fnt: &Fnt,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        let mut width = 0;
        for glyph_index in 0..=255 {
            if !fnt.glyphs.contains_key(&glyph_index) {
                continue;
            }
            let glyph = &fnt.glyphs[&glyph_index];
            width += glyph.width;
        }

        let mut buf = GrayImage::new(width as u32, fnt.height as u32);
        for p in buf.pixels_mut() {
            *p = Luma { data: [1] };
        }

        let mut interp = Interpreter::new();
        interp.add_trampoline(0x60_0000, "finish", 0);
        interp.set_register_value(Reg::EAX, 0);
        interp.set_register_value(Reg::ECX, width as u32);
        interp.set_register_value(Reg::EDI, 0x30_0000);
        interp.map_writable(0x30_0000, buf.into_raw())?;

        let mut x = 0;
        let mut glyph_layout = HashMap::new();
        for glyph_index in 0..=255 {
            if !fnt.glyphs.contains_key(&glyph_index) {
                continue;
            }
            let glyph = &fnt.glyphs[&glyph_index];

            interp.clear_code();
            interp.add_code(&glyph.bytecode);
            interp.push_stack_value(0x60_0000);

            let rv = interp.interpret(0)?;
            let (trampoline_name, args) = rv.ok_trampoline()?;
            ensure!(trampoline_name == "finish", "expect return to finish");
            ensure!(args.is_empty(), "expect no args out");

            glyph_layout.insert(
                glyph_index,
                GlyphLayout {
                    s0: x as f32 / width as f32,
                    s1: (x + glyph.width) as f32 / width as f32,
                    render_width: glyph.width as f32 / 640f32 * 2f32,
                },
            );
            x += glyph.width;
            interp.set_register_value(Reg::EDI, 0x30_0000 + x as u32);
        }

        let plane = interp.unmap_writable(0x30_0000)?;
        let buf =
            GrayImage::from_raw(width as u32, fnt.height as u32, plane).expect("same parameters");
        let buf = buf.expand_palette(&[(0, 0, 0), (0xFF, 0xFF, 0xFF)], Some(1));
        let img = image::ImageRgba8(buf);

        let (texture, tex_future) = Self::upload_texture_rgba(window, img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = Self::make_sampler(window.device())?;

        let pds = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        // The intended display resolution of these fonts was a 640x480 screen.
        let render_height = fnt.height as f32 / 480f32 * 2f32;

        Ok(Self {
            pds,
            glyph_layout,
            render_height,
        })
    }

    fn upload_texture_rgba(
        window: &GraphicsWindow,
        image_buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
    ) -> Fallible<(Arc<ImmutableImage<Format>>, Box<GpuFuture>)> {
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
        trace!(
            "uploading texture with {} bytes",
            image_dim.0 * image_dim.1 * 4
        );
        Ok((texture, Box::new(tex_future) as Box<GpuFuture>))
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
}

pub struct TextRenderer {
    _palette: Rc<Box<Palette>>,
    screen_pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    fonts: HashMap<Font, Rc<Box<FontInfo>>>,
    layouts: Vec<LayoutHandle>,
}

impl TextRenderer {
    pub fn new(
        palette: Rc<Box<Palette>>,
        lib: &Arc<Box<Library>>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
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

        let mut fonts = HashMap::new();
        fonts.insert(
            Font::HUD11,
            Rc::new(Box::new(FontInfo::new_transparent(
                &Fnt::from_bytes("", "", &lib.load("HUD11.FNT")?)?,
                screen_pipeline.clone(),
                window,
            )?)),
        );

        Ok(Self {
            fonts,
            _palette: palette,
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
        let info = self.fonts[&font].clone();
        let layout = LayoutHandle::new(Layout::new(text, info, window)?);
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
    //use crate::ArcBallCamera;
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

            let palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));
            let mut renderer = TextRenderer::new(palette, &lib, &window)?;

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
                renderer.set_projection(&window)?;
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

                window.drive_frame(|command_buffer, dynamic_state| {
                    renderer.render(command_buffer, dynamic_state)
                })?;
            }
        }
        std::mem::drop(window);
        Ok(())
    }
}