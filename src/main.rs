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
use absolute_unit::prelude::*;
use animate::{TimeStep, Timeline};
use anyhow::Result;
use asset_loader::AssetLoader;
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{
    ArcBallController, ArcBallSystem, CameraSystem, ScreenCamera, ScreenCameraController,
};
use composite::CompositeRenderPass;
use event_mapper::EventMapper;
use flight_dynamics::ClassicFlightModel;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu};
use gui::{Gui, GuiStep};
use input::{InputSystem, InputTarget};
use instrument_envelope::EnvelopeInstrument;
use lib::{Libs, LibsOpts};
use marker::Markers;
use measure::{BodyMotion, WorldSpaceFrame};
use nitrous::{inject_nitrous_resource, NitrousResource};
use orrery::Orrery;
use player::PlayerCameraController;
use runtime::{
    ExitRequest, Extension, PlayerMarker, Runtime, StartupOpts, WellKnownPaths, WellKnownPathsOpts,
};
use shape::ShapeBuffer;
use spog::{Dashboard, Terminal};
use stars::StarsBuffer;
use structopt::StructOpt;
use t2_terrain::T2TerrainBuffer;
use terminal_size::{terminal_size, Width};
use terrain::{TerrainBuffer, TerrainOpts};
use tracelog::{TraceLog, TraceLogOpts};
use vehicle::{
    AirbrakeEffector, BayEffector, FlapsEffector, GearEffector, HookEffector, PitchInceptor,
    PowerSystem, RollInceptor, YawInceptor,
};
use window::{DisplayOpts, Window, WindowBuilder};
use world::WorldRenderPass;
use xt::{TypeManager, TypeRef};

const PRELUDE: &str = r#"
// System controls
bindings.bind("Escape", "exit()");
bindings.bind("q", "exit()");
bindings.bind("c", "time.next_time_compression()");
bindings.bind("Control+F3", "dashboard.toggle()");

// Camera Controls
bindings.bind("F1", "@camera.controller.set_mode('Forward')");
bindings.bind("F2", "@camera.controller.set_mode('Backward')");
bindings.bind("F3", "@camera.controller.set_mode('LookUp')");
bindings.bind("F4", "@camera.controller.set_mode('Target')");
bindings.bind("F5", "@camera.controller.set_mode('Incoming')");
bindings.bind("F6", "@camera.controller.set_mode('Wingman')");
bindings.bind("F7", "@camera.controller.set_mode('PlayerToTarget')");
bindings.bind("F8", "@camera.controller.set_mode('TargetToPlayer')");
bindings.bind("F9", "@camera.controller.set_mode('FlyBy')");
bindings.bind("F10", "@camera.controller.set_mode('External')");
bindings.bind("F12", "@camera.controller.set_mode('Missle')");
bindings.bind("+mouse1", "@camera.controller.set_pan_view(pressed)");
bindings.bind("mouseMotion", "@camera.controller.handle_mousemotion(dx, dy)");
bindings.bind("mouseWheel", "@camera.controller.handle_mousewheel(vertical_delta)");

// Flight controls
bindings.bind("key1", "@Player.throttle.set_military(0.)");
bindings.bind("key2", "@Player.throttle.set_military(25.)");
bindings.bind("key3", "@Player.throttle.set_military(50.)");
bindings.bind("key4", "@Player.throttle.set_military(75.)");
bindings.bind("key5", "@Player.throttle.set_military(100.)");
bindings.bind("key6", "@Player.throttle.set_afterburner()");
bindings.bind("b", "@Player.airbrake.toggle()");
bindings.bind("o", "@Player.bay.toggle()");
bindings.bind("f", "@Player.flaps.toggle()");
bindings.bind("h", "@Player.hook.toggle()");
bindings.bind("g", "@Player.gear.toggle()");
bindings.bind("+Up", "@Player.stick_pitch.key_move_front(pressed)");
bindings.bind("+Down", "@Player.stick_pitch.key_move_back(pressed)");
bindings.bind("+Left", "@Player.stick_roll.key_move_left(pressed)");
bindings.bind("+Right", "@Player.stick_roll.key_move_right(pressed)");
bindings.bind("+Comma", "@Player.pedals_yaw.key_move_left(pressed)");
bindings.bind("+Period", "@Player.pedals_yaw.key_move_right(pressed)");
//bindings.bind("joyX", "@Player.elevator.set_position(axis)");

// Debug camera controls
bindings.bind("+mouse1", "@fallback_camera.arcball.pan_view(pressed)");
bindings.bind("+mouse3", "@fallback_camera.arcball.move_view(pressed)");
bindings.bind("mouseMotion", "@fallback_camera.arcball.handle_mousemotion(dx, dy)");
bindings.bind("mouseWheel", "@fallback_camera.arcball.handle_mousewheel(vertical_delta)");
bindings.bind("+Shift+Up", "@fallback_camera.arcball.target_up_fast(pressed)");
bindings.bind("+Shift+Down", "@fallback_camera.arcball.target_down_fast(pressed)");
bindings.bind("+Up", "@fallback_camera.arcball.target_up(pressed)");
bindings.bind("+Down", "@fallback_camera.arcball.target_down(pressed)");

// Camera internal control
bindings.bind("+PageUp", "camera.increase_fov(pressed)");
bindings.bind("+PageDown", "camera.decrease_fov(pressed)");
bindings.bind("Shift+LBracket", "camera.decrease_exposure()");
bindings.bind("Shift+RBracket", "camera.increase_exposure()");

// Load at Mt Everest if nothing else is loaded
game.detach_camera();
let location := "Everest";
@fallback_camera.arcball.set_target(@fallback_camera.arcball.notable_location(location));
@fallback_camera.arcball.set_eye(@fallback_camera.arcball.eye_for(location));
orrery.set_date_time(1964, 2, 24, 12, 0, 0);
"#;

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

#[derive(Debug, Default, NitrousResource)]
struct System {}

impl Extension for System {
    type Opts = ();
    fn init(runtime: &mut Runtime, _: ()) -> Result<()> {
        runtime.insert_named_resource("system", System::default());
        runtime.add_frame_system(
            Self::sys_draw_overlay
                .after(GuiStep::StartFrame)
                .before(GuiStep::EndFrame),
        );
        Ok(())
    }
}

#[inject_nitrous_resource]
impl System {
    fn sys_draw_overlay(
        gui: Res<Gui>,
        player_query: Query<
            (&TypeRef, &BodyMotion, &WorldSpaceFrame, &ClassicFlightModel),
            With<PlayerMarker>,
        >,
        mut query: Query<(&mut ArcBallController, &ScreenCameraController)>,
        mut camera: ResMut<ScreenCamera>,
        mut orrery: ResMut<Orrery>,
    ) {
        if let Ok((xt, motion, frame, flight)) = player_query.get_single() {
            Self::draw_envelope(&gui, xt, motion, frame, flight);
        }
        if let Ok((mut arcball, _)) = query.get_single_mut() {
            Self::draw_info_area(&gui, &mut orrery, &mut arcball, &mut camera);
        }
    }

    fn draw_envelope(
        gui: &Gui,
        xt: &TypeRef,
        motion: &BodyMotion,
        frame: &WorldSpaceFrame,
        flight: &ClassicFlightModel,
    ) {
        let instrument = EnvelopeInstrument::default();
        egui::Area::new("plane_envelope")
            .fixed_pos([
                gui.screen().width() - instrument.display_width(),
                gui.screen().height() - instrument.display_height(),
            ])
            .show(gui.screen().ctx(), |ui| {
                instrument.ui(ui, xt, motion, frame, flight);
            });
    }

    fn draw_info_area(
        gui: &Gui,
        orrery: &mut Orrery,
        arcball: &mut ArcBallController,
        camera: &mut ScreenCamera,
    ) {
        egui::Area::new("info")
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos([gui.screen().width(), 0.])
            .show(gui.screen().ctx(), |ui| {
                egui::Frame::none()
                    .fill(egui::hex_color!("#202020a0"))
                    .stroke(egui::Stroke::new(2., egui::Color32::BLACK))
                    .inner_margin(egui::style::Margin {
                        left: 10.,
                        bottom: 6.,
                        top: 4.,
                        right: 4.,
                    })
                    .rounding(egui::Rounding {
                        sw: 10.,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        egui::CollapsingHeader::new("OpenFA")
                            .show_background(false)
                            .default_open(false)
                            .show(ui, |ui| {
                                egui::Grid::new("controls_grid")
                                    .num_columns(2)
                                    .spacing([4.0, 2.0])
                                    .striped(false)
                                    .show(ui, |ui| {
                                        Self::fill_info_grid(ui, orrery, arcball, camera);
                                    });
                            });
                    });
            });
    }

    fn fill_info_grid(
        ui: &mut egui::Ui,
        orrery: &mut Orrery,
        arcball: &mut ArcBallController,
        camera: &mut ScreenCamera,
    ) {
        ui.label("Date");
        let datetime = orrery.get_time().naive_utc();
        let mut date = datetime.date();
        ui.add(egui_extras::DatePickerButton::new(&mut date));
        if date != datetime.date() {
            orrery.set_date_naive_utc(date);
        }
        ui.end_row();

        ui.label("Time");
        let t = datetime.time();

        ui.label(t.format("%H:%M:%S").to_string())
            .on_hover_text("Middle mouse + drag to change time");
        ui.end_row();

        ui.label("Latitude");
        let mut lat = arcball.target().lat::<Degrees>().f32();
        ui.add(egui::Slider::new(&mut lat, -90.0..=90.0).suffix("°"))
            .on_hover_text("Right click + drag for fine control");
        arcball.target_mut().latitude = radians!(degrees!(lat));
        ui.end_row();

        ui.label("Longitude");
        let mut lon = arcball.target().lon::<Degrees>().f32();
        ui.add(egui::Slider::new(&mut lon, -180.0..=180.0).suffix("°"))
            .on_hover_text("Right click + drag for fine control");
        arcball.target_mut().longitude = radians!(degrees!(lon));
        ui.end_row();

        ui.label("Altitude");
        let mut altitude = feet!(arcball.target().distance).f32();
        ui.add(egui::Slider::new(&mut altitude, 0.0..=40_000.0).suffix("'"))
            .on_hover_text("Up + Down arrows for fine control");
        arcball.target_mut().distance = meters!(feet!(altitude));
        ui.end_row();

        ui.label("FoV").on_hover_text("Field of View");
        let mut fov = degrees!(camera.fov_y()).f32();
        ui.add(egui::Slider::new(&mut fov, 5.0..=120.0).suffix("°"))
            .on_hover_text("Change the field of view");
        camera.set_fov_y(radians!(degrees!(fov)));
        ui.end_row();
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

fn simulation_main(mut runtime: Runtime, opt: Opt) -> Result<()> {
    runtime
        .load_extension::<TimeStep>()?
        .load_extension_with::<WellKnownPaths>(WellKnownPathsOpts::new("openfa"))?
        .load_extension_with::<TraceLog>(opt.tracelog_opts)?
        .load_extension_with::<StartupOpts>(opt.startup_opts.with_prelude(PRELUDE))?
        .load_extension_with::<Libs>(opt.libs_opts)?
        .load_extension::<InputTarget>()?
        .load_extension::<EventMapper>()?
        .load_extension_with::<Window>(opt.display_opts)?
        .load_extension::<Gpu>()?
        .load_extension::<Gui>()?
        .load_extension::<Dashboard>()?
        .load_extension::<Terminal>()?
        .load_extension::<AtmosphereBuffer>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<StarsBuffer>()?
        .load_extension_with::<TerrainBuffer>(TerrainOpts::from_detail(
            opt.detail_opts.cpu_detail(),
            opt.detail_opts.gpu_detail(),
        ))?
        .load_extension::<T2TerrainBuffer>()?
        .load_extension::<WorldRenderPass>()?
        .load_extension::<Markers>()?
        .load_extension::<CompositeRenderPass>()?
        .load_extension::<System>()?
        .load_extension::<Orrery>()?
        .load_extension::<Timeline>()?
        .load_extension::<CameraSystem>()?
        .load_extension::<PlayerCameraController>()?
        .load_extension::<ArcBallSystem>()?
        .load_extension::<TypeManager>()?
        .load_extension::<ShapeBuffer>()?
        .load_extension::<AssetLoader>()?
        .load_extension::<ClassicFlightModel>()?
        .load_extension::<PowerSystem>()?
        .load_extension::<PitchInceptor>()?
        .load_extension::<RollInceptor>()?
        .load_extension::<YawInceptor>()?
        .load_extension::<AirbrakeEffector>()?
        .load_extension::<BayEffector>()?
        .load_extension::<FlapsEffector>()?
        .load_extension::<GearEffector>()?
        .load_extension::<HookEffector>()?;

    // Have an arcball camera controller sitting around that we can fall back to for debugging.
    let _fallback_camera_ent = runtime
        .spawn_named("fallback_camera")?
        .insert(WorldSpaceFrame::default())
        .insert_named(ArcBallController::default())?
        .insert(ScreenCameraController::default())
        .id();

    runtime.run_startup();
    while runtime.resource::<ExitRequest>().still_running() {
        // Catch monotonic sim time up to system time.
        TimeStep::run_sim_loop(&mut runtime);

        // Display a frame
        runtime.run_frame_once();
    }

    Ok(())
}
