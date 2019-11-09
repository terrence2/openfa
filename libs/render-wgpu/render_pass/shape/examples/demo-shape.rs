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
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use nalgebra::Point3;
use omnilib::OmniLib;
use pal::Palette;
use shape_chunk::{DrawSelection, DrawState};
use shape_instance::{
    CoalesceSystem, FlagUpdateSystem, ShapeComponent, ShapeFlagBuffer, ShapeInstanceManager,
    ShapeTransformBuffer, ShapeXformBuffer, TransformUpdateSystem, XformUpdateSystem,
};
use shape_wgpu::ShapeRenderPass;
use specs::prelude::*;
use std::time::Instant;
use world::Transform;

fn main() -> Fallible<()> {
    let omni = OmniLib::new_for_test_in_games(&["FA"])?;
    let lib = omni.library("FA");
    let palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

    let bindings = InputBindings::new("base")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let mut world = World::new();
    world.register::<ShapeComponent>();
    world.register::<ShapeTransformBuffer>();
    world.register::<ShapeFlagBuffer>();
    world.register::<ShapeXformBuffer>();
    world.register::<Transform>();

    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;

    let inst_buffer = ShapeInstanceManager::new(&gpu.device())?;
    const CNT: i32 = 50;
    for x in -CNT / 2..CNT / 2 {
        for y in -CNT / 2..CNT / 2 {
            let (shape_id, slot_id) = inst_buffer.borrow_mut().upload_and_allocate_slot(
                "F18.SH",
                DrawSelection::NormalModel,
                &palette,
                &lib,
                &mut gpu,
            )?;
            let _ent = world
                .create_entity()
                .with(Transform::new(Point3::new(
                    f64::from(x) * 100f64,
                    0f64,
                    f64::from(y) * 100f64,
                )))
                .with(ShapeComponent::new(slot_id, shape_id, DrawState::default()))
                .with(ShapeTransformBuffer::new())
                .with(ShapeFlagBuffer::new(inst_buffer.borrow().errata(shape_id)))
                //.with(ShapeXformBuffer::new())
                .build();
        }
    }
    inst_buffer.borrow_mut().ensure_uploaded(&mut gpu)?;
    gpu.device().poll(true);

    let shape_render_pass =
        ShapeRenderPass::new(&gpu, &globals_buffer.borrow(), &inst_buffer.borrow())?;

    let empty_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &gpu.empty_layout(),
        bindings: &[],
    });

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.1, 3.4e+38);
    camera.set_distance(1500.0);
    camera.on_mousebutton_down(1);

    let start = Instant::now();
    let mut update_dispatcher = DispatcherBuilder::new()
        .with(TransformUpdateSystem, "transform-update", &[])
        .with(FlagUpdateSystem::new(&start), "flag-update", &[])
        .with(XformUpdateSystem::new(&start), "xform-update", &[])
        .build();

    loop {
        let loop_head = Instant::now();
        for command in input.poll()? {
            match command.name.as_str() {
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
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

        let mut upload_buffers = Vec::new();
        globals_buffer
            .borrow()
            .make_upload_buffer(&camera, gpu.device(), &mut upload_buffers)?;
        update_dispatcher.dispatch(&world);
        {
            DispatcherBuilder::new()
                .with(
                    CoalesceSystem::new(&mut inst_buffer.borrow_mut()),
                    "coalesce",
                    &[],
                )
                .build()
                .dispatch(&world);
        }
        inst_buffer
            .borrow()
            .make_upload_buffer(gpu.device(), &mut upload_buffers)?;

        let mut frame = gpu.begin_frame();
        {
            for desc in upload_buffers.drain(..) {
                frame.copy_buffer_to_buffer(
                    &desc.source,
                    desc.source_offset,
                    &desc.destination,
                    desc.destination_offset,
                    desc.copy_size,
                );
            }

            shape_render_pass.render(
                &empty_bind_group,
                &globals_buffer.borrow(),
                &inst_buffer.borrow(),
                &mut frame,
            )?;
        }
        frame.finish();

        println!("frame time: {:?}", loop_head.elapsed());
    }
}
