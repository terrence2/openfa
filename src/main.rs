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
use absolute_unit::{degrees, knots, pounds_force, pounds_mass};
use animate::{TimeStep, Timeline};
use anyhow::{anyhow, Result};
use asset_loader::{AssetLoader, PlayerMarker};
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{ArcBallController, ArcBallSystem, CameraSystem, ScreenCamera};
use composite::CompositeRenderPass;
use csscolorparser::Color;
use event_mapper::EventMapper;
use flight_dynamics::FlightDynamics;
use fnt::Fnt;
use font_fnt::FntFont;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu, GpuStep};
use input::{InputSystem, InputTarget};
use instrument_envelope::EnvelopeInstrument;
use lib::{Libs, LibsOpts};
use marker::Markers;
use measure::{BodyMotion, WorldSpaceFrame};
use nitrous::{inject_nitrous_resource, HeapMut, NitrousResource};
use orrery::Orrery;
use physical_constants::StandardAtmosphere;
use platform_dirs::AppDirs;
use player::PlayerCameraController;
use runtime::{report, ExitRequest, Extension, Runtime, StartupOpts};
use shape::ShapeBuffer;
use stars::StarsBuffer;
use std::{fs::create_dir_all, time::Instant};
use structopt::StructOpt;
use t2_terrain::T2TerrainBuffer;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use tracelog::{TraceLog, TraceLogOpts};
use ui::UiRenderPass;
use vehicle_state::VehicleState;
use widget::{
    FontId, Label, Labeled, LayoutNode, LayoutPacking, PaintContext, Terminal, WidgetBuffer,
};
use window::{size::Size, DisplayOpts, Window, WindowBuilder};
use world::WorldRenderPass;
use xt::TypeManager;

/// Show resources from Jane's Fighters Anthology engine LIB files.
#[derive(Clone, Debug, StructOpt)]
#[structopt(set_term_width = if let Some((Width(w), _)) = terminal_size() { w as usize } else { 80 })]
struct Opt {
    #[structopt(flatten)]
    libs_opts: LibsOpts,

    #[structopt(flatten)]
    detail_opts: DetailLevelOpts,

    #[structopt(flatten)]
    display_opts: DisplayOpts,

    #[structopt(flatten)]
    startup_opts: StartupOpts,

    #[structopt(flatten)]
    tracelog_opts: TraceLogOpts,
}

#[derive(Debug)]
struct VisibleWidgets {
    _demo_label: Entity,
    sim_time: Entity,
    camera_direction: Entity,
    camera_position: Entity,
    camera_fov: Entity,
    fps_label: Entity,

    weight_label: Entity,
    engine_label: Entity,
    accel_label: Entity,
    alpha_label: Entity,
}

#[derive(Debug, NitrousResource)]
struct System {
    visible_widgets: VisibleWidgets,
}

impl Extension for System {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let system = System::new(runtime.heap_mut())?;
        runtime.insert_named_resource("system", system);
        runtime
            .add_frame_system(Self::sys_track_visible_state.after(GpuStep::PresentTargetSurface));
        runtime.run_string(
            r#"
                bindings.bind("Escape", "exit()");
                bindings.bind("q", "exit()");
            "#,
        )?;
        Ok(())
    }
}

#[inject_nitrous_resource]
impl System {
    pub fn new(heap: HeapMut) -> Result<Self> {
        let visible_widgets = Self::build_gui(heap)?;
        let system = Self { visible_widgets };
        Ok(system)
    }

    pub fn build_gui(mut heap: HeapMut) -> Result<VisibleWidgets> {
        let fnt = Fnt::from_bytes(heap.resource::<Libs>().read_name("HUD11.FNT")?.as_ref())?;
        let font = FntFont::from_fnt(&fnt)?;
        heap.resource_mut::<PaintContext>().add_font("HUD11", font);
        let font_id = heap
            .resource::<PaintContext>()
            .font_context
            .font_id_for_name("HUD11");

        let sim_time = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("sim_time", heap.as_mut())?;
        let camera_direction = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("camera_direction", heap.as_mut())?;
        let camera_position = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("camera_position", heap.as_mut())?;
        let camera_fov = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("camera_fov", heap.as_mut())?;
        let mut controls_box = LayoutNode::new_vbox("controls_box", heap.as_mut())?;
        let controls_id = controls_box.id();
        controls_box.push_widget(sim_time)?;
        controls_box.push_widget(camera_direction)?;
        controls_box.push_widget(camera_position)?;
        controls_box.push_widget(camera_fov)?;
        heap.resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_layout(controls_box)?;
        let controls_packing = LayoutPacking::default()
            .float_end()
            .float_top()
            .set_background("#222a")?
            .set_padding_left("10px", heap.as_mut())?
            .set_padding_bottom("6px", heap.as_mut())?
            .set_padding_top("4px", heap.as_mut())?
            .set_padding_right("4px", heap.as_mut())?
            .set_border_color("#000")?
            .set_border_left("2px", heap.as_mut())?
            .set_border_bottom("2px", heap.as_mut())?
            .to_owned();
        *heap.get_mut::<LayoutPacking>(controls_id) = controls_packing;

        fn make_label(name: &str, font_id: FontId, heap: HeapMut) -> Result<Entity> {
            Label::new("empty")
                .with_font(font_id)
                .with_color(&Color::from([0, 255, 0]))
                .with_size(Size::from_pts(12.0))
                .wrapped(name, heap)
        }

        let weight_label = make_label("weight", font_id, heap.as_mut())?;
        let engine_label = make_label("engine", font_id, heap.as_mut())?;
        let accel_label = make_label("accel", font_id, heap.as_mut())?;
        let alpha_label = make_label("alpha", font_id, heap.as_mut())?;

        let mut player_box = LayoutNode::new_vbox("player_box", heap.as_mut())?;
        let player_box_id = player_box.id();
        player_box.push_widget(weight_label)?;
        player_box.push_widget(engine_label)?;
        player_box.push_widget(accel_label)?;
        player_box.push_widget(alpha_label)?;
        heap.resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_layout(player_box)?;
        let player_box_packing = LayoutPacking::default()
            .float_start()
            .float_top()
            .set_background("#222a")?
            .set_padding_right("10px", heap.as_mut())?
            .set_padding_bottom("6px", heap.as_mut())?
            .set_padding_top("4px", heap.as_mut())?
            .set_padding_left("4px", heap.as_mut())?
            .set_border_color("#000")?
            .set_border_right("2px", heap.as_mut())?
            .set_border_bottom("2px", heap.as_mut())?
            .to_owned();
        *heap.get_mut::<LayoutPacking>(player_box_id) = player_box_packing;

        let fps_label = Label::new("")
            .with_font(
                heap.resource::<PaintContext>()
                    .font_context
                    .font_id_for_name("sans"),
            )
            .with_color(&Color::from([255, 0, 0]))
            .with_size(Size::from_pts(13.0))
            .with_pre_blended_text()
            .wrapped("fps_label", heap.as_mut())?;
        heap.resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_widget(fps_label)?;
        heap.get_mut::<LayoutPacking>(fps_label).float_bottom();

        let demo_label = Label::new("").wrapped("demo_label", heap.as_mut())?;

        let mut envelope = EnvelopeInstrument::new(heap.resource::<PaintContext>());
        envelope.set_scale(2.).set_mode("all")?;
        let envelope_id = envelope.wrapped("envelope_instrument", heap.as_mut())?;
        let envelope_packing = LayoutPacking::default()
            .float_end()
            .float_bottom()
            .set_margin_right("20px", heap.as_mut())?
            .set_margin_bottom("20px", heap.as_mut())?
            .to_owned();
        *heap.get_mut::<LayoutPacking>(envelope_id) = envelope_packing;
        heap.resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_widget(envelope_id)?;

        Ok(VisibleWidgets {
            _demo_label: demo_label,
            sim_time,
            camera_direction,
            camera_position,
            camera_fov,
            fps_label,
            weight_label,
            engine_label,
            accel_label,
            alpha_label,
        })
    }

    fn sys_track_visible_state(
        camera: Res<ScreenCamera>,
        timestep: Res<TimeStep>,
        orrery: Res<Orrery>,
        system: Res<System>,
        mut labels: Query<&mut Label>,
        query: Query<
            (
                &WorldSpaceFrame,
                &BodyMotion,
                &VehicleState,
                &FlightDynamics,
            ),
            With<PlayerMarker>,
        >,
    ) {
        report!(system.track_visible_state(&camera, &timestep, &orrery, &mut labels, &query));
    }

    fn track_visible_state(
        &self,
        camera: &ScreenCamera,
        timestep: &TimeStep,
        orrery: &Orrery,
        labels: &mut Query<&mut Label>,
        query: &Query<
            (
                &WorldSpaceFrame,
                &BodyMotion,
                &VehicleState,
                &FlightDynamics,
            ),
            With<PlayerMarker>,
        >,
    ) -> Result<()> {
        labels
            .get_mut(self.visible_widgets.sim_time)?
            .set_text(format!("Date: {}", orrery.get_time()));
        labels
            .get_mut(self.visible_widgets.camera_fov)?
            .set_text(format!("FoV: {}", degrees!(camera.fov_y())));

        if let Ok((frame, motion, vehicle, dynamics)) = query.get_single() {
            labels
                .get_mut(self.visible_widgets.weight_label)?
                .set_text(format!(
                    "Weight: {:0.1}",
                    pounds_mass!(vehicle.current_mass())
                ));
            let altitude = frame.position_graticule().distance;
            let atmosphere = StandardAtmosphere::at_altitude(altitude);
            labels
                .get_mut(self.visible_widgets.engine_label)?
                .set_text(format!(
                    "Engine: {} ({:0.0})",
                    vehicle.power_plant().engine_display(),
                    pounds_force!(vehicle.power_plant().forward_thrust(&atmosphere, motion))
                ));
            labels
                .get_mut(self.visible_widgets.accel_label)?
                .set_text(format!(
                    "Accel: {:0.4}",
                    motion.vehicle_forward_acceleration()
                ));
            labels
                .get_mut(self.visible_widgets.alpha_label)?
                .set_text(format!("Alpha: {:0.2}", degrees!(dynamics.alpha())));
            labels
                .get_mut(self.visible_widgets.camera_direction)?
                .set_text(format!("V: {:0.4}", knots!(motion.cg_velocity())));
            labels
                .get_mut(self.visible_widgets.camera_position)?
                .set_text(format!("Position: {:0.4}", frame.position(),));
        }
        let frame_time = timestep.now().elapsed();
        let ts = format!(
            "frame: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        labels.get_mut(self.visible_widgets.fps_label)?.set_text(ts);
        Ok(())
    }
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    env_logger::init();
    InputSystem::run_forever(
        opt,
        WindowBuilder::new().with_title("OpenFA"),
        simulation_main,
    )
}

fn simulation_main(mut runtime: Runtime) -> Result<()> {
    let opt = runtime.resource::<Opt>().to_owned();

    let app_dirs = AppDirs::new(Some("openfa"), true)
        .ok_or_else(|| anyhow!("unable to find app directories"))?;
    create_dir_all(&app_dirs.config_dir)?;
    create_dir_all(&app_dirs.state_dir)?;

    runtime
        .insert_resource(opt.libs_opts)
        .insert_resource(opt.display_opts)
        .insert_resource(opt.startup_opts)
        .insert_resource(opt.tracelog_opts)
        .insert_resource(opt.detail_opts.cpu_detail())
        .insert_resource(opt.detail_opts.gpu_detail())
        .insert_resource(app_dirs)
        .load_extension::<TraceLog>()?
        .load_extension::<StartupOpts>()?
        .load_extension::<Libs>()?
        .load_extension::<InputTarget>()?
        .load_extension::<EventMapper>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        .load_extension::<AtmosphereBuffer>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<StarsBuffer>()?
        .load_extension::<TerrainBuffer>()?
        .load_extension::<T2TerrainBuffer>()?
        .load_extension::<WorldRenderPass>()?
        .load_extension::<WidgetBuffer>()?
        .load_extension::<UiRenderPass>()?
        .load_extension::<Markers>()?
        .load_extension::<CompositeRenderPass>()?
        .load_extension::<System>()?
        .load_extension::<Label>()?
        .load_extension::<Terminal>()?
        .load_extension::<Orrery>()?
        .load_extension::<Timeline>()?
        .load_extension::<TimeStep>()?
        .load_extension::<CameraSystem>()?
        .load_extension::<PlayerCameraController>()?
        .load_extension::<ArcBallSystem>()?
        .load_extension::<TypeManager>()?
        .load_extension::<ShapeBuffer>()?
        .load_extension::<AssetLoader>()?
        .load_extension::<VehicleState>()?
        .load_extension::<FlightDynamics>()?
        .load_extension::<EnvelopeInstrument>()?;

    // Have an arcball camera controller sitting around that we can fall back to for debugging.
    let _fallback_camera_ent = runtime
        .spawn_named("fallback_camera")?
        .insert(WorldSpaceFrame::default())
        .insert_named(ArcBallController::default())?
        .id();

    runtime.run_startup();
    while runtime.resource::<ExitRequest>().still_running() {
        // Catch monotonic sim time up to system time.
        let frame_start = Instant::now();
        while runtime.resource::<TimeStep>().next_now() < frame_start {
            runtime.run_sim_once();
        }

        // Display a frame
        runtime.run_frame_once();
    }

    Ok(())
}
