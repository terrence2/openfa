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
use anyhow::{anyhow, Result};
use asset_loader::AssetLoader;
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{
    ArcBallController, ArcBallSystem, CameraSystem, ScreenCamera, ScreenCameraController,
};
use catalog::{Catalog, DirectoryDrawer};
use composite::CompositeRenderPass;
use egui::hex_color;
use event_mapper::EventMapper;
use flight_dynamics::ClassicFlightModel;
use fullscreen::FullscreenBuffer;
use geodesy::Graticule;
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu};
use gui::{Gui, GuiStep};
use input::{InputSystem, InputTarget};
use lib::{Libs, LibsOpts};
use marker::{EntityMarkers, Markers};
use measure::WorldSpaceFrame;
use nitrous::{inject_nitrous_resource, method, HeapMut, NitrousResource};
use once_cell::sync::Lazy;
use orrery::Orrery;
use player::PlayerCameraController;
use runtime::{ExitRequest, Extension, PlayerMarker, Runtime, WellKnownPaths, WellKnownPathsOpts};
use shape::{ShapeBuffer, ShapeId, ShapeScale};
use spog::{Dashboard, Terminal};
use stars::StarsBuffer;
use std::time::Duration;
use structopt::StructOpt;
use t2_terrain::T2TerrainBuffer;
use terminal_size::{terminal_size, Width};
use terrain::{TerrainBuffer, TerrainOpts};
use tracelog::{TraceLog, TraceLogOpts};
use vehicle::{
    AirbrakeControl, AirbrakeEffector, Airframe, BayControl, BayEffector, FlapsControl,
    FlapsEffector, FuelSystem, FuelTank, FuelTankKind, GearControl, GearEffector, GliderEngine,
    HookControl, HookEffector, PitchInceptor, PowerSystem, RollInceptor, ThrottleInceptor,
    YawInceptor,
};
use window::{DisplayOpts, Window, WindowBuilder};
use world::WorldRenderPass;
use xt::TypeManager;

const LATITUDE: f64 = 58.287_f64;
const LONGITUDE: f64 = -25.641_f64;

static PRELUDE: Lazy<String> = Lazy::new(|| {
    format!(
        r#"
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
bindings.bind("key6", "@Player.throttle.set_afterburner(0)");
bindings.bind("b", "@Player.airbrake.toggle()");
bindings.bind("f", "@Player.flaps.toggle()");
bindings.bind("h", "@Player.hook.toggle()");
bindings.bind("o", "@Player.bay.toggle()");
bindings.bind("g", "@Player.gear.toggle()");
bindings.bind("+Up", "@Player.stick_y.key_move_forward(pressed)");
bindings.bind("+Down", "@Player.stick_y.key_move_backward(pressed)");
bindings.bind("+Left", "@Player.ailerons.move_stick_left(pressed)");
bindings.bind("+Right", "@Player.ailerons.move_stick_right(pressed)");
bindings.bind("+Comma", "@Player.rudder.move_pedals_left(pressed)");
bindings.bind("+Period", "@Player.rudder.move_pedals_right(pressed)");
//bindings.bind("joyX", "@Player.elevator.set_position(axis)");
bindings.bind("n", "system.toggle_show_normals()");
bindings.bind("Shift+Slash", "system.toggle_show_help()");
bindings.bind("F1", "system.toggle_show_help()");

// Debug camera controls
bindings.bind("+mouse1", "@fallback_camera.arcball.pan_view(pressed)");
bindings.bind("+mouse3", "@fallback_camera.arcball.move_view(pressed)");
bindings.bind("mouseMotion", "@fallback_camera.arcball.handle_mousemotion(dx, dy)");
bindings.bind("mouseWheel", "@fallback_camera.arcball.handle_mousewheel(vertical_delta)");
bindings.bind("+Shift+Up", "@fallback_camera.arcball.target_up_fast(pressed)");
bindings.bind("+Shift+Down", "@fallback_camera.arcball.target_down_fast(pressed)");
bindings.bind("+Up", "@fallback_camera.arcball.target_up(pressed)");
bindings.bind("+Down", "@fallback_camera.arcball.target_down(pressed)");

// Load at Mt Everest if nothing else is loaded
game.detach_camera();
game.load_map("BAL.MM");
@fallback_camera.arcball.set_target_latitude_degrees({});
@fallback_camera.arcball.set_target_longitude_degrees({});
@fallback_camera.arcball.set_target_height_meters(6096.0);
@fallback_camera.arcball.set_eye_latitude_degrees(14.94270422048709);
@fallback_camera.arcball.set_eye_longitude_degrees(260.0);
@fallback_camera.arcball.set_eye_distance_meters(101.60619925904378);

// Camera internal control
bindings.bind("+PageUp", "camera.increase_fov(pressed)");
bindings.bind("+PageDown", "camera.decrease_fov(pressed)");
bindings.bind("Shift+LBracket", "camera.decrease_exposure()");
bindings.bind("Shift+RBracket", "camera.increase_exposure()");

// Let there be light
orrery.set_unix_ms(13459754321.0);
"#,
        LATITUDE, LONGITUDE
    )
});

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
    tracelog_opts: TraceLogOpts,

    /// One SH file to view
    inputs: Vec<String>,
}

#[derive(Debug, Default, NitrousResource)]
struct System {
    showing_normals: bool,
    showing_help: bool,
}

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
    #[method]
    fn toggle_show_help(&mut self) {
        self.showing_help = !self.showing_help;
    }

    #[method]
    fn toggle_show_normals(&mut self, mut heap: HeapMut) -> Result<()> {
        let id = heap.entity_by_name("Player");
        if self.showing_normals {
            heap.get_mut::<EntityMarkers>(id).clear_body_arrows();
            self.showing_normals = false;
            return Ok(());
        }
        self.showing_normals = true;

        let shape_id = *heap.get::<ShapeId>(id);
        let verts = heap
            .resource::<ShapeBuffer>()
            .read_back_vertices(shape_id, heap.resource::<Gpu>())?;
        for (i, vert) in verts.iter().enumerate() {
            heap.get_mut::<EntityMarkers>(id).add_body_arrow(
                &format!("n-{}", i),
                vert.point().map(|v| meters!(v)),
                vert.normal().map(|v| meters!(v * 10f32)),
                meters!(0.25_f64),
                "#F0F".parse()?,
            );
        }

        Ok(())
    }

    fn sys_draw_overlay(
        system: Res<System>,
        gui: Res<Gui>,
        mut query: Query<(&mut ArcBallController, &ScreenCameraController)>,
        mut camera: ResMut<ScreenCamera>,
        mut orrery: ResMut<Orrery>,
        plane_query: Query<
            (
                &mut ThrottleInceptor,
                &PowerSystem,
                &mut GearControl,
                &GearEffector,
                &mut BayControl,
                &BayEffector,
                &mut AirbrakeControl,
                &AirbrakeEffector,
                &mut FlapsControl,
                &FlapsEffector,
                &mut HookControl,
                &HookEffector,
            ),
            With<PlayerMarker>,
        >,
    ) {
        Self::draw_plane_display_status(&gui, plane_query);
        if system.showing_help {
            Self::draw_help(&gui);
        }
        if let Ok((mut arcball, _)) = query.get_single_mut() {
            Self::draw_info_area(&gui, &mut orrery, &mut arcball, &mut camera);
        }
    }

    fn draw_plane_display_status(
        gui: &Gui,
        plane_query: Query<
            (
                &mut ThrottleInceptor,
                &PowerSystem,
                &mut GearControl,
                &GearEffector,
                &mut BayControl,
                &BayEffector,
                &mut AirbrakeControl,
                &AirbrakeEffector,
                &mut FlapsControl,
                &FlapsEffector,
                &mut HookControl,
                &HookEffector,
            ),
            With<PlayerMarker>,
        >,
    ) {
        egui::Area::new("plane_info")
            .pivot(egui::Align2::LEFT_TOP)
            .fixed_pos([0., 0.])
            .show(gui.screen().ctx(), |ui| {
                egui::Frame::none()
                    .fill(egui::hex_color!("#202020a0"))
                    .stroke(egui::Stroke::new(2., egui::Color32::BLACK))
                    .inner_margin(egui::style::Margin {
                        left: 4.,
                        bottom: 6.,
                        top: 4.,
                        right: 10.,
                    })
                    .rounding(egui::Rounding {
                        se: 10.,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        egui::Grid::new("plane_grid")
                            .num_columns(3)
                            .spacing([4.0, 2.0])
                            .striped(false)
                            .show(ui, |ui| {
                                Self::fill_plane_grid(ui, plane_query);
                            });
                    });
            });
    }

    fn fill_plane_grid(
        ui: &mut egui::Ui,
        mut plane_query: Query<
            (
                &mut ThrottleInceptor,
                &PowerSystem,
                &mut GearControl,
                &GearEffector,
                &mut BayControl,
                &BayEffector,
                &mut AirbrakeControl,
                &AirbrakeEffector,
                &mut FlapsControl,
                &FlapsEffector,
                &mut HookControl,
                &HookEffector,
            ),
            With<PlayerMarker>,
        >,
    ) {
        let (
            mut throttle,
            power,
            mut gear_control,
            gear_effector,
            mut bay_control,
            bay_effector,
            mut airbrake_control,
            airbrake_effector,
            mut flaps_control,
            flaps_effector,
            mut hook_control,
            hook_effector,
        ) = plane_query.single_mut();

        fn t(s: &str) -> egui::RichText {
            egui::RichText::new(s)
                .text_style(egui::TextStyle::Monospace)
                .color(egui::Color32::GREEN)
                .size(16.)
        }

        ui.label(t("Gear:"));
        let mut control = gear_control.is_enabled();
        ui.checkbox(&mut control, "");
        ui.label(t(&format!("{:0.0}%", gear_effector.position() * 100.)));
        gear_control.set_enabled(control);
        ui.end_row();

        ui.label(t("Bay:"));
        let mut control = bay_control.is_enabled();
        ui.checkbox(&mut control, "");
        ui.label(t(&format!("{:0.0}%", bay_effector.position() * 100.)));
        bay_control.set_enabled(control);
        ui.end_row();

        ui.label(t("Airbrake:"));
        let mut control = airbrake_control.is_enabled();
        ui.checkbox(&mut control, "");
        ui.label(t(&format!("{:0.0}%", airbrake_effector.position() * 100.)));
        airbrake_control.set_enabled(control);
        ui.end_row();

        ui.label(t("Flaps:"));
        let mut control = flaps_control.is_enabled();
        ui.checkbox(&mut control, "");
        ui.label(t(&format!("{:0.0}%", flaps_effector.position() * 100.)));
        flaps_control.set_enabled(control);
        ui.end_row();

        ui.label(t("Hook:"));
        let mut control = hook_control.is_enabled();
        ui.checkbox(&mut control, "");
        ui.label(t(&format!("{:0.0}%", hook_effector.position() * 100.)));
        hook_control.set_enabled(control);
        ui.end_row();

        ui.label(t("Engine:"));
        let mut military = throttle.position().military();
        let mut afterburner = throttle.position().is_afterburner();
        ui.style_mut().spacing.slider_width = 30.;
        ui.vertical(|ui| {
            ui.checkbox(&mut afterburner, "AFT");
            ui.add(
                egui::Slider::new(&mut military, 0.0..=100.0)
                    .orientation(egui::SliderOrientation::Vertical)
                    .show_value(false),
            );
        });
        ui.label(t(&format!("{}", power.engine(0).current_power())));
        if afterburner {
            throttle.set_afterburner();
        } else {
            throttle.set_military(military);
        }
        ui.end_row();
    }

    fn draw_help(gui: &Gui) {
        const HELP: &str = r#"
F1, ?        - show or hide this help text
left mouse   - change view angle
middle mouse - change time of day
right mouse  - move view position
f            - toggle flaps
g            - toggle gear
b            - toggle airbrake
o            - toggle bay
h            - toggle hook
1-5          - turn off afterburner
6            - turn on afterburner
n            - toggle normals display
"#;
        egui::Window::new("How to use this program:")
            .frame(
                egui::Frame::window(&gui.screen().ctx().style())
                    .shadow(egui::epaint::Shadow::NONE)
                    .fill(hex_color!("202020A0")),
            )
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::default())
            .show(gui.screen().ctx(), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(HELP)
                            .text_style(egui::TextStyle::Monospace)
                            .color(egui::Color32::GREEN)
                            .size(18.),
                    )
                    .wrap(false),
                );
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
        .id();

    // Find the shape file to show. Add the parent as a libdir. Use the final component as the
    // shape name for lookup in the new libdir. Note that we need libs handy to find PALETTE.PAL
    // so we need a higher priority on our new libdir.
    let files = Libs::input_files(&opt.inputs, "*.SH")?;
    let shape_file = files
        .first()
        .ok_or_else(|| anyhow!("Must be run with a shape input"))?;
    let shape_file = shape_file.canonicalize()?;
    let shape_name = shape_file.file_name().unwrap().to_string_lossy();
    println!("Loading Shape: {:?}", shape_file);
    runtime
        .resource_mut::<Catalog>()
        .add_drawer(DirectoryDrawer::from_directory(
            i64::MAX,
            shape_file.parent().unwrap(),
        )?)?;
    runtime.resource_scope(|heap, mut shapes: Mut<ShapeBuffer>| {
        shapes.upload_shapes(
            heap.resource::<Libs>().palette(),
            &[&shape_name],
            heap.resource::<Catalog>(),
            heap.resource::<Gpu>(),
        )
    })?;
    let shape_id = runtime
        .resource::<ShapeBuffer>()
        .shape_ids_for_preloaded_shape(shape_name)?
        .normal();

    let frame = WorldSpaceFrame::from_graticule(
        Graticule::new(
            degrees!(LATITUDE),
            degrees!(LONGITUDE),
            meters!(feet!(20_000.0_f64)),
        ),
        Graticule::new(
            degrees!(0_f64),
            degrees!(260_f64 + 90_f64),
            meters!(feet!(200.0_f64)),
        ),
    );
    let fuel = FuelSystem::default()
        .with_internal_tank(FuelTank::new(FuelTankKind::Center, kilograms!(0_f64)))?;
    let power = PowerSystem::default().with_engine(GliderEngine::default());
    let shape_ent = runtime
        .spawn_named("Player")?
        .insert(PlayerMarker)
        .insert(ShapeScale::new(1.))
        .insert_named(frame)?
        .insert_named(Airframe::new(kilograms!(10_f64)))?
        .insert_named(fuel)?
        .insert_named(power)?
        .insert_named(PitchInceptor::default())?
        .insert_named(RollInceptor::default())?
        .insert_named(YawInceptor::default())?
        .insert_named(ThrottleInceptor::new_min_power())?
        .insert_named(AirbrakeControl::default())?
        .insert_named(AirbrakeEffector::new(0., Duration::from_millis(1)))?
        .insert_named(BayControl::default())?
        .insert_named(BayEffector::new(0., Duration::from_secs(2)))?
        .insert_named(FlapsControl::default())?
        .insert_named(FlapsEffector::new(0., Duration::from_millis(1)))?
        .insert_named(GearControl::default())?
        .insert_named(GearEffector::new(0., Duration::from_secs(4)))?
        .insert_named(HookControl::default())?
        .insert_named(HookEffector::new(0., Duration::from_millis(1)))?
        .insert_named(EntityMarkers::default())?
        .id();
    runtime.resource_scope(|mut heap, mut shapes: Mut<ShapeBuffer>| {
        heap.resource_scope(|mut heap, gpu: Mut<Gpu>| {
            let entity = heap.named_entity_mut(shape_ent);
            shapes.instantiate(entity, shape_id, &gpu)
        })
    })?;

    runtime.run_string(&PRELUDE)?;
    runtime.run_startup();
    while runtime.resource::<ExitRequest>().still_running() {
        // Catch monotonic sim time up to system time.
        TimeStep::run_sim_loop(&mut runtime);

        // Display a frame
        runtime.run_frame_once();
    }

    Ok(())
}
