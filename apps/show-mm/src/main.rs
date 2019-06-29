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
use asset::AssetManager;
use camera::ArcBallCamera;
use failure::{bail, Fallible};
use input::{InputBindings, InputSystem};
use log::trace;
use mm::MissionMap;
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use render::{PalRenderer, T2Renderer};
use simplelog::{Config, LevelFilter, TermLogger};
use std::{rc::Rc, sync::Arc, time::Instant};
use structopt::StructOpt;
use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use window::{GraphicsConfigBuilder, GraphicsWindow};
use xt::TypeManager;

make_opt_struct!(
    #[structopt(name = "mm_explorer", about = "Show the contents of an MM file")]
    Opt {}
);

pub fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    let (omni, inputs) = opt.find_inputs()?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, name) = inputs.first().unwrap();
    let lib = omni.library(&game);

    let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    let shape_bindings = InputBindings::new("map")
        .bind("+pan-view", "mouse1")?
        .bind("+move-view", "mouse3")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?
        .bind("reset", "r")?
        .bind("layer-base-up", "t")?
        .bind("layer-base-down", "g")?
        .bind("c2-up", "y")?
        .bind("c2-down", "h")?
        .bind("d3-up", "u")?
        .bind("d3-down", "j")?
        .bind("e0-up", "i")?
        .bind("e0-down", "k")?
        .bind("f1-up", "o")?
        .bind("f1-down", "l")?;
    let mut input = InputSystem::new(&[&shape_bindings]);

    let assets = Arc::new(Box::new(AssetManager::new(lib.clone())?));
    let types = TypeManager::new(lib.clone());

    let contents = lib.load_text(&name)?;
    let mm = MissionMap::from_str(&contents, &types)?;

    let mut text_renderer = TextRenderer::new(system_palette, &lib, &window)?;
    let fps_handle = text_renderer
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom);
    let state_handle = text_renderer
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0.5f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Right)
        .with_horizontal_anchor(TextAnchorH::Right)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom);

    ///////////////////////////////////////////////////////////
    let mut t2_renderer = T2Renderer::new(mm, &assets, &lib, &window)?;
    let mut lay_base = -3;
    let mut e0_off = -1;
    let mut f1_off = -1;
    let mut c2_off = 0;
    let mut d3_off = 0;
    t2_renderer.set_palette_parameters(&window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
    let mut pal_renderer = PalRenderer::new(&window)?;
    pal_renderer.update_pal_data(&t2_renderer.used_palette, &window)?;
    ///////////////////////////////////////////////////////////

    let mut camera = ArcBallCamera::new(window.aspect_ratio()?, 0.001f32, 3.4e+38f32);

    let mut need_reset = false;
    loop {
        let loop_start = Instant::now();

        if need_reset {
            need_reset = false;
            t2_renderer
                .set_palette_parameters(&window, lay_base, e0_off, f1_off, c2_off, d3_off)?;
            pal_renderer.update_pal_data(&t2_renderer.used_palette, &window)?;
        }

        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio()?);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => camera.on_mousemove(
                    command.displacement()?.0 as f32,
                    command.displacement()?.1 as f32,
                ),
                "mouse-wheel" => camera.on_mousescroll(
                    command.displacement()?.0 as f32,
                    command.displacement()?.1 as f32,
                ),
                "+pan-view" => camera.on_mousebutton_down(1),
                "-pan-view" => camera.on_mousebutton_up(1),
                "+move-view" => camera.on_mousebutton_down(3),
                "-move-view" => camera.on_mousebutton_up(3),
                "reset" => need_reset = true,
                "layer-base-up" => lay_base += 1,
                "layer-base-down" => lay_base -= 1,
                "c2-up" => c2_off += 1,
                "c2-down" => c2_off -= 1,
                "d3-up" => d3_off += 1,
                "d3-down" => d3_off -= 1,
                "e0-up" => e0_off += 1,
                "e0-down" => e0_off -= 1,
                "f1-up" => f1_off += 1,
                "f1-down" => f1_off -= 1,
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            text_renderer.before_frame(&window)?;
            t2_renderer.before_frame(&camera)?;

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = t2_renderer.render(cbb, &window.dynamic_state)?;
            cbb = text_renderer.render(cbb, &window.dynamic_state)?;
            cbb = pal_renderer.render(cbb, &window.dynamic_state)?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }

        let offsets = format!(
            "base: lay:{} c2:{} d3:{} e0:{} f1:{}",
            lay_base, c2_off, d3_off, e0_off, f1_off
        );
        state_handle.set_span(&offsets, &window)?;

        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        fps_handle.set_span(&ts, &window)?;
    }
}