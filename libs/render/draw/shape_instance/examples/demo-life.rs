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
use rand::prelude::*;
use shape_chunk::DrawSelection;
use shape_instance::{
    ShapeRenderSystem, ShapeRenderer, ShapeUpdateFlagSystem, ShapeUpdateTransformSystem,
};
use specs::{world::Index as EntityId, DispatcherBuilder};
use std::{sync::Arc, time::Instant};
use vulkano::{command_buffer::AutoCommandBufferBuilder, sync::GpuFuture};
use window::{GraphicsConfigBuilder, GraphicsWindow};
use world::World;

fn main() -> Fallible<()> {
    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;
    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(&[&bindings]);
    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let lib = omni.library("FA");
    let world = Arc::new(World::new(lib)?);

    let mut shape_renderer = ShapeRenderer::new(world.clone(), &window)?;
    let (_f18_id, fut1) =
        shape_renderer.upload_shape("F18.SH", DrawSelection::NormalModel, &window)?;
    let (shape_id, fut1) =
        shape_renderer.upload_shape("WNDMLL.SH", DrawSelection::NormalModel, &window)?;
    let future = shape_renderer.ensure_uploaded(&window)?;

    assert!(fut1.is_none());
    future.then_signal_fence_and_flush()?.wait(None)?;

    const D: f64 = 120f64;
    let mut rng = rand::thread_rng();
    const WIDTH: usize = 100;
    const HEIGHT: usize = 100;
    let mut life = [[None; WIDTH]; HEIGHT];
    for i in 0..WIDTH {
        for j in 0..HEIGHT {
            life[i][j] = if rng.gen::<f32>() > 0.5 {
                let shape_part = shape_renderer.chunks().part(shape_id);
                Some(world.create_flyer(
                    shape_id,
                    Point3::new(-50f64 * D + D * i as f64, 0f64, -50f64 * D + D * j as f64),
                    shape_part,
                )?)
            } else {
                None
            };
        }
    }

    let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
    camera.set_distance(12000.0);
    camera.on_mousebutton_down(1);

    let mut cnt = 0;
    window.hide_cursor()?;
    loop {
        let frame_start = Instant::now();

        cnt += 1;
        for command in input.poll(&mut window.events_loop) {
            match command.name.as_str() {
                "window-resize" => {
                    window.note_resize();
                    camera.set_aspect_ratio(window.aspect_ratio_f64()?);
                }
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "mouse-wheel" => {
                    camera.on_mousescroll(command.displacement()?.0, command.displacement()?.1)
                }
                "mouse-move" => camera.on_mousemove(
                    command.displacement()?.0 / 4.0,
                    command.displacement()?.1 / 4.0,
                ),
                "window-cursor-move" => {}
                _ => println!("unhandled command: {}", command.name),
            }
        }
        window.center_cursor()?;

        //if cnt % 10 == 0 {
        if false {
            let start = std::time::Instant::now();
            let mut next = [[0u8; WIDTH]; HEIGHT];
            for i in 0..WIDTH {
                for j in 0..HEIGHT {
                    let mut neighbors = 0;
                    if i > 0 && j > 0 && life[i - 1][j - 1].is_some() {
                        neighbors += 1;
                    }
                    if i > 0 && life[i - 1][j].is_some() {
                        neighbors += 1;
                    }
                    if i > 0 && j < HEIGHT - 1 && life[i - 1][j + 1].is_some() {
                        neighbors += 1;
                    }
                    if j > 0 && life[i][j - 1].is_some() {
                        neighbors += 1;
                    }
                    if j < HEIGHT - 1 && life[i][j + 1].is_some() {
                        neighbors += 1;
                    }
                    if i < WIDTH - 1 && j > 0 && life[i + 1][j - 1].is_some() {
                        neighbors += 1;
                    }
                    if i < WIDTH - 1 && life[i + 1][j].is_some() {
                        neighbors += 1;
                    }
                    if i < WIDTH - 1 && j < HEIGHT - 1 && life[i + 1][j + 1].is_some() {
                        neighbors += 1;
                    }
                    next[i][j] = neighbors as u8;
                }
            }
            for i in 0..WIDTH {
                for j in 0..HEIGHT {
                    let neighbors = next[i][j];
                    if neighbors < 2 || neighbors > 3 {
                        if let Some(ent) = life[i][j] {
                            world.destroy_entity(ent)?;
                            life[i][j] = None;
                        }
                    } else if neighbors == 3 && life[i][j].is_none() {
                        let shape_part = shape_renderer.chunks().part(shape_id);
                        life[i][j] = Some(world.create_flyer(
                            shape_id,
                            Point3::new(-50f64 * D + D * i as f64, 0f64, -50f64 * D + D * j as f64),
                            shape_part,
                        )?);
                    }
                }
            }
            println!("TICK: @{} => {:?}", cnt, start.elapsed());
        }

        // Upload entities' current state to the renderer.
        let dup = Instant::now();
        {
            //let shape_render_system = ShapeRenderSystem::new(&mut shape_renderer);
            //let mut shape_instance_updater = DispatcherBuilder::new()
            //    .with(shape_render_system, "", &[])
            //    .build();
            //world.run(&mut shape_instance_updater);
            let mut disp = DispatcherBuilder::new()
                .with(ShapeUpdateTransformSystem, "transform", &[])
                .with(ShapeUpdateFlagSystem, "flag", &[])
                .build();
            world.run(&mut disp);
        }
        println!("DUP: {:?}", dup.elapsed());

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            let update = Instant::now();
            cbb = shape_renderer.update_buffers(cbb)?;
            println!("UPDATE: {:?}", update.elapsed());

            cbb = cbb.begin_render_pass(
                frame.framebuffer(&window),
                false,
                vec![[0f32, 0f32, 1f32, 1f32].into(), 0f32.into()],
            )?;

            cbb = shape_renderer.render(cbb, &camera, &window)?;

            cbb = cbb.end_render_pass()?;

            let cb = cbb.build()?;

            frame.submit(cb, &mut window)?;
        }

        let maint = Instant::now();
        shape_renderer.maintain();
        println!("MAINT: {:?}", maint.elapsed());

        println!("FRAME: {:?}", frame_start.elapsed());
    }
}
