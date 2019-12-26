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
use absolute_unit::meters;
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use failure::{bail, Fallible};
use frame_graph::make_frame_graph;
use fullscreen::FullscreenBuffer;
use galaxy::{Galaxy, FEET_TO_HM_32};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputBindings, InputSystem};
use log::trace;
use mm::MissionMap;
use nalgebra::Vector3;
use omnilib::{make_opt_struct, OmniLib};
use screen_text::ScreenTextRenderPass;
use shape::ShapeRenderPass;
use shape_instance::{DrawSelection, ShapeInstanceBuffer};
use simplelog::{Config, LevelFilter, TermLogger};
use skybox::SkyboxRenderPass;
use stars::StarsBuffer;
use std::time::Instant;
use structopt::StructOpt;
use t2_buffer::T2Buffer;
use t2_terrain::T2TerrainRenderPass;
use text_layout::{Font, LayoutBuffer, TextAnchorH, TextAnchorV, TextPositionH, TextPositionV};
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
            screen_text: ScreenTextRenderPass { globals, text_layout }
        ];
    }
);

fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Warn, Config::default())?;

    let (omni, game, name) = opt.find_input(&opt.omni_input)?;
    let lib = omni.library(&game);
    let mut galaxy = Galaxy::new(lib)?;

    let mm_bindings = InputBindings::new("map")
        .bind("prev-object", "Shift+n")?
        .bind("next-object", "n")?;
    let system_bindings = InputBindings::new("map")
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![
        ArcBallCamera::default_bindings()?,
        mm_bindings,
        system_bindings,
    ])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let types = TypeManager::new(galaxy.library_owned());
    let mm = MissionMap::from_str(
        &galaxy.library().load_text(&name)?,
        &types,
        galaxy.library(),
    )?;

    let mut position_index = 0;
    let mut positions = Vec::new();
    let mut names = Vec::new();
    let t2_buffer = T2Buffer::new(&mm, galaxy.palette(), galaxy.library(), &mut gpu)?;

    let shape_instance_buffer = ShapeInstanceBuffer::new(gpu.device())?;
    {
        for info in mm.objects() {
            if info.xt().ot().shape.is_none() {
                // FIXME: this still needs to add the entity.
                // I believe these are only for hidden flak guns in TVIET.
                continue;
            }

            let (shape_id, slot_id) = shape_instance_buffer
                .borrow_mut()
                .upload_and_allocate_slot(
                    info.xt().ot().shape.as_ref().expect("a shape file"),
                    DrawSelection::NormalModel,
                    galaxy.palette(),
                    galaxy.library(),
                    &mut gpu,
                )?;
            let aabb = *shape_instance_buffer
                .borrow()
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
                let ns_ft = t2_buffer.borrow().t2().extent_north_south_in_ft();
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
                    shape_instance_buffer.borrow().part(shape_id),
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
    shape_instance_buffer
        .borrow_mut()
        .ensure_uploaded(&mut gpu)?;

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(gpu.device())?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(gpu.device())?;
    let text_layout_buffer = LayoutBuffer::new(galaxy.library(), &mut gpu)?;

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

    let mut camera = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.1), meters!(3.4e+38));
    camera.set_target_point(&nalgebra::convert(positions[position_index]));

    loop {
        let loop_start = Instant::now();

        for command in input.poll()? {
            camera.handle_command(&command)?;
            match command.name.as_str() {
                // system bindings
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(&input);
                    camera.set_aspect_ratio(gpu.aspect_ratio());
                }

                // mm bindings
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

                _ => trace!("unhandled command: {}", command.name),
            }
        }

        let sun_direction = Vector3::new(
            camera.sun_angle.sin() as f32,
            0f32,
            camera.sun_angle.cos() as f32,
        );

        let mut buffers = Vec::new();
        globals_buffer
            .borrow()
            .make_upload_buffer_for_arcball_in_tile(
                t2_buffer.borrow().t2(),
                &camera,
                &gpu,
                &mut buffers,
            )?;
        atmosphere_buffer
            .borrow()
            .make_upload_buffer(sun_direction, gpu.device(), &mut buffers)?;
        shape_instance_buffer.borrow_mut().make_upload_buffer(
            &galaxy.start_owned(),
            galaxy.world_mut(),
            gpu.device(),
            &mut buffers,
        )?;
        text_layout_buffer
            .borrow()
            .make_upload_buffer(&gpu, &mut buffers)?;
        frame_graph.run(&mut gpu, buffers)?;

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
