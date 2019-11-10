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
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use failure::{bail, Fallible};
use frame_graph::make_frame_graph;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use log::trace;
use mm::MissionMap;
use nalgebra::Vector3;
use omnilib::{make_opt_struct, OmniLib};
use screen_text::ScreenTextRenderPass;
use shape::ShapeRenderPass;
use shape_instance::{CoalesceSystem, FlagUpdateSystem, TransformUpdateSystem, XformUpdateSystem};
use shape_instance::{DrawSelection, ShapeInstanceBuffer};
use simplelog::{Config, LevelFilter, TermLogger};
use skybox::SkyboxRenderPass;
use specs::prelude::*;
use stars::StarsBuffer;
use std::time::Instant;
use structopt::StructOpt;
use t2_buffer::T2Buffer;
use t2_terrain::T2TerrainRenderPass;
use text_layout::{Font, LayoutBuffer, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV};
use universe::{Universe, FEET_TO_HM};
use xt::TypeManager;

make_opt_struct!(
    #[structopt(name = "mm_explorer", about = "Show the contents of an MM file")]
    Opt {
        #[structopt(help = "MM file to load")]
        omni_input => String
    }
);

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            shape_instance_buffer: ShapeInstanceBuffer,
            stars: StarsBuffer,
            t2: T2Buffer,
            text_layout: LayoutBuffer
        };
        passes: [
            skybox: SkyboxRenderPass { globals, fullscreen, stars, atmosphere },
            terrain: T2TerrainRenderPass { globals, atmosphere, t2 },
            shape: ShapeRenderPass { globals, shape_instance_buffer },
            screen_text: ScreenTextRenderPass { text_layout }
        ];
    }
);

pub fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Warn, Config::default())?;

    let (omni, game, name) = opt.find_input(&opt.omni_input)?;
    let lib = omni.library(&game);
    let mut universe = Universe::new(lib)?;

    let shape_bindings = InputBindings::new("map")
        .bind("prev-object", "Shift+n")?
        .bind("next-object", "n")?
        .bind("+pan-view", "mouse1")?
        .bind("+move-view", "mouse3")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![shape_bindings])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let types = TypeManager::new(universe.library_owned());
    let mm = MissionMap::from_str(
        &universe.library().load_text(&name)?,
        &types,
        universe.library(),
    )?;

    let mut position_index = 0;
    let mut positions = Vec::new();
    let mut names = Vec::new();
    let shape_instance_buffer = ShapeInstanceBuffer::new(gpu.device())?;
    {
        for info in mm.objects() {
            let (shape_id, slot_id) = shape_instance_buffer
                .borrow_mut()
                .upload_and_allocate_slot(
                    info.xt().ot().shape.as_ref().expect("a shape file"),
                    DrawSelection::NormalModel,
                    universe.palette(),
                    universe.library(),
                    &mut gpu,
                )?;

            if let Ok(pt) = info.xt().pt() {
                //universe.create_flyer(pt, shape_id, slot_id)?
                //unimplemented!()
            } else if let Ok(nt) = info.xt().nt() {
                //universe.create_ground_mover(nt)
                //unimplemented!()
            } else if let Ok(jt) = info.xt().jt() {
                bail!("did not expect a projectile in MM objects")
            } else {
                println!("Obj Inst {:?}: {:?}", info.xt().ot().shape, info.position());
                let mut p = info.position();
                p *= FEET_TO_HM;
                positions.push(p);
                let sh_name = info
                    .xt()
                    .ot()
                    .shape
                    .as_ref()
                    .expect("a shape file")
                    .to_owned();
                if let Some(n) = info.name() {
                    names.push(n + " (" + &sh_name + ")");
                } else {
                    names.push(sh_name);
                }
                universe.create_building(
                    slot_id,
                    shape_id,
                    shape_instance_buffer.borrow().part(shape_id),
                    p,
                    //nalgebra::Point3::new(i, i, i),
                )?;
            };
        }
    }
    shape_instance_buffer
        .borrow_mut()
        .ensure_uploaded(&mut gpu)?;

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(gpu.device())?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(gpu.device())?;
    let t2_buffer = T2Buffer::new(mm, universe.palette(), universe.library(), &mut gpu)?;
    let text_layout_buffer = LayoutBuffer::new(universe.library(), &mut gpu)?;

    let frame_graph = FrameGraph::new(
        &mut gpu,
        &atmosphere_buffer,
        &fullscreen_buffer,
        &globals_buffer,
        &shape_instance_buffer,
        &stars_buffer,
        &t2_buffer,
        &text_layout_buffer,
    )?;
    ///////////////////////////////////////////////////////////

    let fps_handle = text_layout_buffer
        .borrow_mut()
        .add_screen_text(Font::HUD11, "", gpu.device())?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom);

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), 0.001, 3.4e+38);

    loop {
        let loop_start = Instant::now();

        for command in input.poll()? {
            match command.name.as_str() {
                "prev-object" => {
                    if position_index > 0 {
                        position_index -= 1;
                    }
                    camera.set_target_point(&nalgebra::convert(positions[position_index]));
                }
                "next-object" => {
                    if position_index < positions.len() - 1 {
                        position_index += 1;
                    }
                    camera.set_target_point(&nalgebra::convert(positions[position_index]));
                }
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

        // 51°30′26″N 0°7′39″W
        // let london_lon = (7.0 / 60.0) + (39.0 / 3600.0);
        // let london_lat = 51.0 + (30.0 / 60.0) + (26.0 / 3600.0);

        let start = Instant::now();
        let mut shape_buffer_update_dispatcher = DispatcherBuilder::new()
            .with(TransformUpdateSystem, "transform-update", &[])
            .with(FlagUpdateSystem::new(&start), "flag-update", &[])
            .with(XformUpdateSystem::new(&start), "xform-update", &[])
            .build();

        shape_buffer_update_dispatcher.dispatch(&universe.ecs);
        {
            DispatcherBuilder::new()
                .with(
                    CoalesceSystem::new(&mut shape_instance_buffer.borrow_mut()),
                    "coalesce",
                    &[],
                )
                .build()
                .dispatch(&universe.ecs);
        }

        let sun_direction = Vector3::new(0f32, 1f32, 0f32);

        let mut buffers = Vec::new();
        globals_buffer
            .borrow()
            .make_upload_buffer(&camera, gpu.device(), &mut buffers)?;
        atmosphere_buffer
            .borrow()
            .make_upload_buffer(sun_direction, gpu.device(), &mut buffers)?;
        shape_instance_buffer
            .borrow()
            .make_upload_buffer(gpu.device(), &mut buffers)?;
        text_layout_buffer
            .borrow()
            .make_upload_buffer(&gpu, &mut buffers)?;
        frame_graph.run(&mut gpu, buffers);

        let ft = loop_start.elapsed();
        let ts = format!(
            "@{} {} - {}.{} ms",
            position_index,
            names[position_index],
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        fps_handle.set_span(&ts, gpu.device())?;
    }
}
