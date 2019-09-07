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
use shape_chunk::DrawSelection;
use shape_instance::{ShapeRenderSystem, ShapeRenderer};
use specs::DispatcherBuilder;
use std::sync::Arc;
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
    let (f8_id, fut1) =
        shape_renderer.upload_shape("F8.SH", DrawSelection::NormalModel, &window)?;
    let (f18_id, fut2) =
        shape_renderer.upload_shape("F18.SH", DrawSelection::NormalModel, &window)?;
    //let (soldier_id, fut2) =
    //    shape_renderer.upload_shape("SOLDIER.SH", DrawSelection::NormalModel, &window)?;
    let (_windmill_id, fut3) =
        shape_renderer.upload_shape("WNDMLL.SH", DrawSelection::NormalModel, &window)?;
    let future = shape_renderer.ensure_uploaded(&window)?;

    assert!(fut1.is_none());
    assert!(fut2.is_none());
    assert!(fut3.is_none());
    future.then_signal_fence_and_flush()?.wait(None)?;

    let _f18_ent1 = world.create_flyer(f18_id, Point3::new(0f64, 0f64, 0f64))?;
    let _f18_ent2 = world.create_flyer(f8_id, Point3::new(80f64, 0f64, 120f64))?;

    let mut camera = ArcBallCamera::new(window.aspect_ratio_f64()?, 0.1, 3.4e+38);
    camera.set_distance(120.0);
    camera.on_mousebutton_down(1);

    window.hide_cursor()?;
    loop {
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
        window.center_cursor()?;

        // Upload entities' current state to the renderer.
        {
            let shape_render_system = ShapeRenderSystem::new(&mut shape_renderer);
            let mut shape_instance_updater = DispatcherBuilder::new()
                .with(shape_render_system, "", &[])
                .build();
            world.run(&mut shape_instance_updater);
        }

        {
            let frame = window.begin_frame()?;
            if !frame.is_valid() {
                continue;
            }

            let mut cbb = AutoCommandBufferBuilder::primary_one_time_submit(
                window.device(),
                window.queue().family(),
            )?;

            cbb = shape_renderer.update_buffers(cbb)?;

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

        shape_renderer.maintain();
    }
}
