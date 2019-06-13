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
use dlg::Dialog;
use failure::{bail, Fallible};
use input::{InputBindings, InputSystem};
use log::trace;
use omnilib::{make_opt_struct, OmniLib};
use pal::Palette;
use render::DialogRenderer;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{rc::Rc, sync::Arc, time::Instant};
use structopt::StructOpt;
use text::{Font, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV, TextRenderer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use window::{GraphicsConfigBuilder, GraphicsWindow};

make_opt_struct!(#[structopt(
    name = "dlg_explorer",
    about = "Show the contents of a DLG file"
)]
Opt {
    #[structopt(short = "b", long = "background", help = "The background for this dialog")]
    background => Option<String>
});

pub fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    let (omni, inputs) = opt.find_inputs()?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let (game, name) = inputs.first().unwrap();
    let lib = omni.library(&game);

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    let bindings = InputBindings::new("menu")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(&[&bindings]);

    let system_palette = Rc::new(Box::new(Palette::from_bytes(&lib.load("PALETTE.PAL")?)?));
    let mut text_renderer = TextRenderer::new(system_palette.clone(), &lib, &window)?;
    let fps_handle = text_renderer
        .add_screen_text(Font::HUD11, "", &window)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top);

    let background = if let Some(s) = opt.background {
        s
    } else {
        "CHOOSEAC.PIC".to_owned()
    };
    let dlg = Arc::new(Box::new(Dialog::from_bytes(&lib.load(name)?)?));

    ///////////////////////////////////////////////////////////
    let mut dlg_renderer = DialogRenderer::new(dlg, &background, &lib, &window)?;
    ///////////////////////////////////////////////////////////

    loop {
        let loop_start = Instant::now();

        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => window.note_resize(),
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                _ => trace!("unhandled command: {}", command.name),
            }
        }

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            dlg_renderer.before_frame(&window)?;
            text_renderer.before_frame(&window)?;

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = dlg_renderer.render(cbb, &window.dynamic_state)?;
            cbb = text_renderer.render(cbb, &window.dynamic_state)?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }

        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        fps_handle.set_span(&ts, &window)?;
    }
}
