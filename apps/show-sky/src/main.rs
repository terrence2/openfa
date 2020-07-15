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
use absolute_unit::{degrees, kilometers, meters};
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use chrono::prelude::*;
use command::Bindings;
use failure::Fallible;
use frame_graph::make_frame_graph;
use fullscreen::FullscreenBuffer;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::InputSystem;
use log::trace;
use nalgebra::convert;
use orrery::Orrery;
use physical_constants::EARTH_RADIUS_KM;
use screen_text::ScreenTextRenderPass;
use simplelog::{Config, LevelFilter, TermLogger};
use skybox::SkyboxRenderPass;
use stars::StarsBuffer;
use std::time::Instant;
use terrain::TerrainRenderPass;
use terrain_geo::{CpuDetailLevel, GpuDetailLevel, TerrainGeoBuffer};
use text_layout::{TextAnchorH, TextAnchorV, TextLayoutBuffer, TextPositionH, TextPositionV};

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            stars: StarsBuffer,
            terrain_geo: TerrainGeoBuffer,
            text_layout: TextLayoutBuffer
        };
        precompute: { terrain_geo };
        renderers: [
            skybox: SkyboxRenderPass { globals, fullscreen, stars, atmosphere },
            terrain: TerrainRenderPass { globals, atmosphere, terrain_geo },
            screen_text: ScreenTextRenderPass { globals, text_layout }
        ];
    }
);

fn main() -> Fallible<()> {
    TermLogger::init(LevelFilter::Warn, Config::default())?;

    let system_bindings = Bindings::new("map")
        .bind("+target_up", "Up")?
        .bind("+target_down", "Down")?
        .bind("exit", "Escape")?
        .bind("exit", "q")?;
    let mut input = InputSystem::new(vec![
        Orrery::debug_bindings()?,
        ArcBallCamera::default_bindings()?,
        system_bindings,
    ])?;
    let mut gpu = GPU::new(&input, Default::default())?;

    let (cpu_detail, gpu_detail) = if cfg!(debug_assertions) {
        (CpuDetailLevel::Low, GpuDetailLevel::Low)
    } else {
        (CpuDetailLevel::Medium, GpuDetailLevel::High)
    };

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(&gpu)?;
    let terrain_geo_buffer = TerrainGeoBuffer::new(cpu_detail, gpu_detail, &mut gpu)?;
    let text_layout_buffer = TextLayoutBuffer::new(&mut gpu)?;

    let mut frame_graph = FrameGraph::new(
        &mut gpu,
        &atmosphere_buffer,
        &fullscreen_buffer,
        &globals_buffer,
        &stars_buffer,
        &terrain_geo_buffer,
        &text_layout_buffer,
    )?;
    ///////////////////////////////////////////////////////////

    let fps_handle = text_layout_buffer
        .borrow_mut()
        .add_screen_text("", "", &gpu)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top)
        .handle();

    let mut orrery = Orrery::new(Utc.ymd(1964, 2, 24).and_hms(12, 0, 0));

    /*
    let mut camera = UfoCamera::new(gpu.aspect_ratio(), 0.1f64, 3.4e+38f64);
    camera.set_position(6_378.0, 0.0, 0.0);
    camera.set_rotation(&Vector3::new(0.0, 0.0, 1.0), PI / 2.0);
    camera.apply_rotation(&Vector3::new(0.0, 1.0, 0.0), PI);
    */

    let mut arcball = ArcBallCamera::new(
        gpu.aspect_ratio(),
        meters!(0.5),
        // FIXME: return our near precision
        meters!(kilometers!(EARTH_RADIUS_KM * 2.0 * 4.0)),
        //meters!(kilometers!(EARTH_RADIUS_KM * 2.0 * 1.3)),
    );
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(2),
    ));
    arcball.set_eye_relative(Graticule::<Target>::new(
        degrees!(89),
        degrees!(0),
        meters!(4_000_000),
        // meters!(1_400_000),
    ))?;

    let mut target_vec = meters!(0f64);
    loop {
        let loop_start = Instant::now();

        for command in input.poll()? {
            arcball.handle_command(&command)?;
            orrery.handle_command(&command)?;
            match command.name.as_str() {
                "+target_up" => target_vec = meters!(1),
                "-target_up" => target_vec = meters!(0),
                "+target_down" => target_vec = meters!(-1),
                "-target_down" => target_vec = meters!(0),
                // system bindings
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(&input);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-cursor-move" => {}
                _ => trace!("unhandled command: {}", command.name),
            }
        }
        let mut g = arcball.get_target();
        g.distance += target_vec;
        if g.distance < meters!(0f64) {
            g.distance = meters!(0f64);
        }
        arcball.set_target(g);

        arcball.think();

        globals_buffer.borrow().make_upload_buffer(
            arcball.camera(),
            &gpu,
            frame_graph.tracker_mut(),
        )?;
        atmosphere_buffer.borrow().make_upload_buffer(
            convert(orrery.sun_direction()),
            &gpu,
            frame_graph.tracker_mut(),
        )?;
        terrain_geo_buffer.borrow_mut().make_upload_buffer(
            arcball.camera(),
            &gpu,
            frame_graph.tracker_mut(),
        )?;
        text_layout_buffer
            .borrow_mut()
            .make_upload_buffer(&gpu, frame_graph.tracker_mut())?;
        frame_graph.run(&mut gpu)?;

        let frame_time = loop_start.elapsed();
        let ts = format!(
            "eye_rel: {} | asl: {}, fov: {} || Date: {:?} || frame: {}.{}ms",
            arcball.get_eye_relative(),
            g.distance,
            degrees!(arcball.camera().fov_y()),
            orrery.get_time(),
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        fps_handle
            .grab(&mut text_layout_buffer.borrow_mut())
            .set_span(&ts);
    }
}
