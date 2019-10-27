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
use failure::Fallible;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use log::trace;
use mm::MissionMap;
use nalgebra::Vector3;
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{rc::Rc, sync::Arc, time::Instant};
use structopt::StructOpt;
use t2_buffer::T2Buffer;
use terrain_t2::TerrainT2RenderPass;
// use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use xt::TypeManager;

make_opt_struct!(
    #[structopt(name = "mm_explorer", about = "Show the contents of an MM file")]
    Opt {
        #[structopt(help = "MM file to load")]
        omni_input => String
    }
);

pub fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    let (omni, game, name) = opt.find_input(&opt.omni_input)?;
    let lib = omni.library(&game);

    let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));
    let shape_bindings = InputBindings::new("map")
        .bind("+pan-view", "mouse1")?
        .bind("+move-view", "mouse3")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![shape_bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let assets = Arc::new(Box::new(AssetManager::new(lib.clone())?));
    let types = TypeManager::new(lib.clone());

    let contents = lib.load_text(&name)?;
    let mm = MissionMap::from_str(&contents, &types, &lib)?;

    /*
    let mut text_renderer = TextRenderer::new(&lib, &window)?;
    let fps_handle = text_renderer
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom);
    */

    ///////////////////////////////////////////////////////////
    let t2_buffer = T2Buffer::new(mm, &system_palette, &assets, &lib, &mut gpu)?;

    let t2_render_pass = TerrainT2RenderPass::new(&mut gpu, t2_buffer)?;
    ///////////////////////////////////////////////////////////

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.001, 3.4e+38);

    loop {
        let _loop_start = Instant::now();

        for command in input.poll()? {
            match command.name.as_str() {
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => {
                    camera.on_mousemove(command.displacement()?.0, command.displacement()?.1)
                }
                "mouse-wheel" => {
                    camera.on_mousescroll(command.displacement()?.0, command.displacement()?.1)
                }
                "+pan-view" => camera.on_mousebutton_down(1),
                "-pan-view" => camera.on_mousebutton_up(1),
                "+move-view" => camera.on_mousebutton_down(3),
                "-move-view" => camera.on_mousebutton_up(3),
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        let upload =
            t2_render_pass.prepare_upload(&camera, &Vector3::new(0f32, 1f32, 0f32), gpu.device());

        {
            let mut frame = gpu.begin_frame();
            {
                t2_render_pass.upload(&mut frame, upload);

                let mut rpass = frame.begin_render_pass();
                t2_render_pass.draw(&mut rpass);
            }
            frame.finish();
        }

        /*
        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        fps_handle.set_span(&ts, &window)?;
        */
    }
}
