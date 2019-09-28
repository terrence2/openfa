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
use camera::ArcBallCamera;
use failure::Fallible;
use input::{InputBindings, InputSystem};
use nalgebra::Point3;
use omnilib::OmniLib;
use pal::Palette;
use shape::ShapeRenderer;
use shape_chunk::DrawSelection;
use shape_instance::{CoalesceSystem, FlagUpdateSystem, TransformUpdateSystem, XformUpdateSystem};
use specs::prelude::*;
use std::time::Instant;
use vulkano::{command_buffer::AutoCommandBufferBuilder, sync::GpuFuture};
use window::{GraphicsConfigBuilder, GraphicsWindow};

fn main() -> Fallible<()> {
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    window.hide_cursor()?;
    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(&[&bindings]);

    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let lib = omni.library("FA");
    let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

    let mut world = World::new();
    let mut shape_renderer = ShapeRenderer::new(&mut world, &window)?;

    const CNT: i32 = 100;
    for x in -CNT / 2..CNT / 2 {
        for y in -CNT / 2..CNT / 2 {
            shape_renderer.add_instance(
                "F18.SH",
                DrawSelection::NormalModel,
                Point3::new(f64::from(x) * 10f64, f64::from(y) * 10f64, 0f64),
                &palette,
                &lib,
                &mut world,
                &window,
            )?;
        }
    }

    if let Some(future) = shape_renderer.ensure_uploaded(&window)? {
        future.then_signal_fence_and_flush()?.wait(None)?;
    }

    let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
    camera.set_distance(120.0);
    camera.on_mousebutton_down(1);

    let start = Instant::now();
    let mut update_dispatcher = DispatcherBuilder::new()
        .with(TransformUpdateSystem, "transform-update", &[])
        .with(FlagUpdateSystem::new(&start), "flag-update", &[])
        .with(XformUpdateSystem::new(&start), "xform-update", &[])
        .build();

    loop {
        let loop_head = Instant::now();
        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio_f64()?);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-move" => camera.on_mousemove(
                    command.displacement()?.0 / 4.0,
                    command.displacement()?.1 / 4.0,
                ),
                "window-cursor-move" => {}
                _ => println!("unhandled command: {}", command.name),
            }
        }
        let center_cursor_head = Instant::now();
        window.center_cursor()?;
        println!("center_cursor time: {:?}", center_cursor_head.elapsed());

        let frame = window.begin_frame()?;
        if !frame.is_valid() {
            continue;
        }

        //let update_head = Instant::now();
        update_dispatcher.dispatch(&world);
        {
            /*
            DispatcherBuilder::new()
                .with(CoalesceSystem::new(&mut inst_man), "coalesce", &[])
                .build()
                .dispatch(&world);
            */
        }
        //println!("update time: {:?}", update_head.elapsed());

        let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
            window.device(),
            window.queue().family(),
        )?;

        //let upload_head = Instant::now();
        cbb = shape_renderer.upload_buffers(cbb)?;
        //println!("upload time: {:?}", upload_head.elapsed());

        cbb = cbb.begin_render_pass(
            frame.framebuffer(&window),
            false,
            vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
        )?;

        //let render_head = Instant::now();
        cbb = shape_renderer.render(cbb, &camera, &window)?;
        //println!("render time: {:?}", render_head.elapsed());

        cbb = cbb.end_render_pass()?;
        let cb = cbb.build()?;
        frame.submit(cb, &mut window)?;

        println!("Frame time: {:?}", loop_head.elapsed());
    }
}
