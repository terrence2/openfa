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
use absolute_unit::{degrees, meters};
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use command::Bindings;
use failure::{bail, Fallible};
use fnt::Font;
use fullscreen::FullscreenBuffer;
use galaxy::Galaxy;
use geodesy::{GeoSurface, Graticule};
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, GPU};
use input::{InputController, InputSystem};
use legion::prelude::*;
use lib::{from_dos_string, CatalogBuilder};
use log::trace;
use mm::MissionMap;
use nalgebra::convert;
use orrery::Orrery;
use physical_constants::FEET_TO_HM_32;
use screen_text::ScreenTextRenderPass;
use shape::ShapeRenderPass;
use shape_instance::{DrawSelection, ShapeInstanceBuffer};
use stars::StarsBuffer;
use std::time::Instant;
use structopt::StructOpt;
use t2_buffer::T2Buffer;
// use t2_terrain::T2TerrainRenderPass;
use text_layout::{TextAnchorH, TextAnchorV, TextLayoutBuffer, TextPositionH, TextPositionV};
use winit::window::Window;
use xt::TypeManager;

/// Show the contents of an MM file
#[derive(Debug, StructOpt)]
struct Opt {
    /// Map to show
    inputs: Vec<String>,
}

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            shape_instance_buffer: ShapeInstanceBuffer,
            stars: StarsBuffer,
            t2: T2Buffer,
            text_layout: TextLayoutBuffer
        };
        renderers: [
            //skybox: SkyboxRenderPass { globals, fullscreen, stars, atmosphere },
            //terrain: T2TerrainRenderPass { globals, atmosphere, t2 },
            shape: ShapeRenderPass { globals, atmosphere, shape_instance_buffer },
            screen_text: ScreenTextRenderPass { globals, text_layout }
        ];
        passes: [
            draw: Render(Screen) {
                shape( globals, atmosphere, shape_instance_buffer ),
                screen_text( globals, text_layout )
            }
        ];
    }
);

fn main() -> Fallible<()> {
    env_logger::init();

    let mm_bindings = Bindings::new("map")
        .bind("mm.prev-object", "Shift+n")?
        .bind("mm.next-object", "n")?;
    let system_bindings = Bindings::new("map")
        .bind("system.exit", "Escape")?
        .bind("system.exit", "q")?;
    InputSystem::run_forever(
        vec![
            Orrery::debug_bindings()?,
            ArcBallCamera::default_bindings()?,
            mm_bindings,
            system_bindings,
        ],
        window_main,
    )
}

fn window_main(window: Window, input_controller: &InputController) -> Fallible<()> {
    let opt = Opt::from_args();
    let (mut catalog, inputs) = CatalogBuilder::build_and_select(&opt.inputs)?;
    if inputs.is_empty() {
        bail!("no inputs");
    }
    let fid = *inputs.first().unwrap();

    //let mut async_rt = Runtime::new()?;
    let mut legion = World::default();

    let label = catalog.file_label(fid)?;
    catalog.set_default_label(&label);
    let meta = catalog.stat_sync(fid)?;
    let name = meta.name;
    let mut galaxy = Galaxy::new(&catalog)?;

    let mut gpu = GPU::new(&window, Default::default())?;

    let types = TypeManager::empty();
    let mm = MissionMap::from_str(
        &from_dos_string(catalog.read_name_sync(&name)?),
        &types,
        &catalog,
    )?;

    let mut position_index = 0;
    let mut positions = Vec::new();
    let mut names = Vec::new();
    let t2_buffer = T2Buffer::new(&mm, galaxy.palette(), &catalog, &mut gpu)?;

    let mut shape_instance_buffer = ShapeInstanceBuffer::new(gpu.device())?;
    {
        for info in mm.objects() {
            if info.xt().ot().shape.is_none() {
                // FIXME: this still needs to add the entity.
                // I believe these are only for hidden flak guns in TVIET.
                continue;
            }

            let (shape_id, slot_id) = shape_instance_buffer.upload_and_allocate_slot(
                info.xt().ot().shape.as_ref().expect("a shape file"),
                DrawSelection::NormalModel,
                galaxy.palette(),
                &catalog,
                &mut gpu,
            )?;
            let aabb = *shape_instance_buffer
                .part(shape_id)
                .widgets()
                .read()
                .unwrap()
                .aabb();

            if let Ok(_pt) = info.xt().pt() {
                //galaxy.create_flyer(pt, shape_id, slot_id)?
                //unimplemented!()
            } else if let Ok(_nt) = info.xt().nt() {
                //galaxy.create_ground_mover(nt)
                //unimplemented!()
            } else if info.xt().jt().is_ok() {
                bail!("did not expect a projectile in MM objects")
            } else {
                println!("Obj Inst {:?}: {:?}", info.xt().ot().shape, info.position());
                let scale = if info
                    .xt()
                    .ot()
                    .shape
                    .as_ref()
                    .expect("a shape file")
                    .starts_with("BNK")
                {
                    2f32
                } else {
                    4f32
                };
                let mut p = info.position();
                let ns_ft = t2_buffer.t2().extent_north_south_in_ft();
                p.coords[2] = ns_ft - p.coords[2]; // flip z for vulkan
                p *= FEET_TO_HM_32;
                p.coords[1] = /*t2_buffer.borrow().ground_height_at_tile(&p)*/
                    -aabb[1][1] * scale * FEET_TO_HM_32;
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
                galaxy.create_building(
                    slot_id,
                    shape_id,
                    shape_instance_buffer.part(shape_id),
                    scale,
                    p,
                    info.angle(),
                )?;
            };
        }

        /*
        let (shape_id, slot_id) = shape_instance_buffer
            .borrow_mut()
            .upload_and_allocate_slot(
                "TREEA.SH",
                DrawSelection::NormalModel,
                galaxy.palette(),
                galaxy.library(),
                &mut gpu,
            )?;
        let height = shape_instance_buffer
            .borrow()
            .part(shape_id)
            .widgets()
            .read()
            .unwrap()
            .height();
        use nalgebra::Point3;
        use rand::distributions::{IndependentSample, Range};
        let ns_between = Range::new(
            0f32,
            t2_buffer.borrow().t2().extent_north_south_in_ft()
                / t2_buffer.borrow().t2().height() as f32,
        );
        let we_between = Range::new(
            0f32,
            t2_buffer.borrow().t2().extent_east_west_in_ft()
                / t2_buffer.borrow().t2().width() as f32,
        );
        let mut rng = rand::thread_rng();
        for i in 0..10000 {
            let x = we_between.ind_sample(&mut rng);
            let z = ns_between.ind_sample(&mut rng);
            let mut p = Point3::new(x, 0f32, z);
            p *= FEET_TO_HM_32;
            p.coords[1] = /*t2_buffer.borrow().ground_height_at_tile(&p)*/
                - height * 4.0 * FEET_TO_HM_32 / 2f32;
            println!("p: {:?}", p);
            galaxy.create_building(
                slot_id,
                shape_id,
                shape_instance_buffer.borrow().part(shape_id),
                p,
                &UnitQuaternion::identity(),
            )?;
        }
        */
    }
    shape_instance_buffer.ensure_uploaded(&mut gpu)?;

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(false, &mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(&gpu)?;
    let text_layout_buffer = TextLayoutBuffer::new(&mut gpu)?;

    let mut frame_graph = FrameGraph::new(
        &mut legion,
        &mut gpu,
        atmosphere_buffer,
        fullscreen_buffer,
        globals_buffer,
        shape_instance_buffer,
        stars_buffer,
        t2_buffer,
        text_layout_buffer,
    )?;
    ///////////////////////////////////////////////////////////

    let fps_handle = frame_graph
        .text_layout
        .add_screen_text(Font::HUD11.name(), "", &gpu)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Bottom)
        .with_vertical_anchor(TextAnchorV::Bottom)
        .handle();

    let mut orrery = Orrery::now();
    let mut arcball = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.1));
    //camera.set_target_point(&nalgebra::convert(positions[position_index]));
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(0),
    ));

    loop {
        let loop_start = Instant::now();

        for command in input_controller.poll()? {
            arcball.handle_command(&command)?;
            orrery.handle_command(&command)?;
            match command.command() {
                // system bindings
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(&window);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }

                // mm bindings
                "prev-object" => {
                    if position_index > 0 {
                        position_index -= 1;
                    }
                    //camera.set_target_point(&nalgebra::convert(positions[position_index]));
                }
                "next-object" => {
                    if position_index < positions.len() - 1 {
                        position_index += 1;
                    }
                    //camera.set_target_point(&nalgebra::convert(positions[position_index]));
                }

                _ => trace!("unhandled command: {}", command.full()),
            }
        }

        let mut tracker = Default::default();
        arcball.think();

        frame_graph
            .globals
            //.make_upload_buffer_for_arcball_in_tile(
            .make_upload_buffer(
                //t2_buffer.borrow().t2(),
                arcball.camera(),
                2.2,
                &gpu,
                &mut tracker,
            )?;
        frame_graph.atmosphere.make_upload_buffer(
            convert(orrery.sun_direction()),
            &gpu,
            &mut tracker,
        )?;
        frame_graph.shape_instance_buffer.make_upload_buffer(
            &galaxy.start_time_owned(),
            galaxy.world_mut(),
            &gpu,
            &mut tracker,
        )?;
        frame_graph
            .text_layout
            .make_upload_buffer(&gpu, &mut tracker)?;
        frame_graph.run(&mut gpu, tracker)?;

        let ft = loop_start.elapsed();
        let ts = format!(
            "@{} {} - {}.{} ms",
            position_index,
            names[position_index],
            ft.as_secs() * 1000 + u64::from(ft.subsec_millis()),
            ft.subsec_micros()
        );
        fps_handle.grab(&mut frame_graph.text_layout).set_span(&ts);
    }
}
