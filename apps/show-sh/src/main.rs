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
use absolute_unit::{degrees, feet, kilograms, meters};
use animate::{TimeStep, Timeline};
use anyhow::{anyhow, Result};
use asset_loader::AssetLoader;
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{ArcBallController, ArcBallSystem, CameraSystem, ScreenCamera};
use catalog::{Catalog, DirectoryDrawer};
use composite::CompositeRenderPass;
use csscolorparser::Color;
use event_mapper::EventMapper;
use flight_dynamics::ClassicFlightModel;
use fnt::Fnt;
use font_fnt::FntFont;
use fullscreen::FullscreenBuffer;
use geodesy::Graticule;
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu, GpuStep};
use input::{InputSystem, InputTarget};
use instrument_envelope::EnvelopeInstrument;
use lib::{Libs, LibsOpts};
use marker::{EntityMarkers, Markers};
use measure::WorldSpaceFrame;
use nitrous::{inject_nitrous_resource, method, HeapMut, NitrousResource};
use once_cell::sync::Lazy;
use orrery::Orrery;
use platform_dirs::AppDirs;
use player::PlayerCameraController;
use runtime::{report, ExitRequest, Extension, PlayerMarker, Runtime};
use shape::{ShapeBuffer, ShapeId, ShapeScale};
use stars::StarsBuffer;
use std::{fs::create_dir_all, time::Duration};
use structopt::StructOpt;
use t2_terrain::T2TerrainBuffer;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use tracelog::{TraceLog, TraceLogOpts};
use ui::UiRenderPass;
use vehicle::{
    AirbrakeControl, AirbrakeEffector, Airframe, BayControl, BayEffector, FlapsControl,
    FlapsEffector, FuelSystem, FuelTank, FuelTankKind, GearControl, GearEffector, GliderEngine,
    HookControl, HookEffector, PitchInceptor, PowerSystem, RollInceptor, ThrottleInceptor,
    YawInceptor,
};
use widget::{
    Label, Labeled, LayoutMeasurements, LayoutNode, LayoutPacking, PaintContext, Terminal,
    WidgetBuffer,
};
use window::{size::Size, DisplayOpts, Window, WindowBuilder};
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
orrery.set_unix_ms(13459754321.0);
// orrery.set_unix_ms(1662157226046.4653
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

#[derive(Debug)]
struct VisibleWidgets {
    sim_time: Entity,
    camera_fov: Entity,
    fps_label: Entity,

    engine_label: Entity,
    airbrake_label: Entity,
    bay_label: Entity,
    flaps_label: Entity,
    gear_label: Entity,
    hook_label: Entity,

    help_box_id: Entity,
    help_line_ids: Vec<Entity>,
}

#[derive(Debug, NitrousResource)]
struct System {
    showing_normals: bool,
    showing_help: bool,
    visible_widgets: VisibleWidgets,
}

impl Extension for System {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let system = System::new(runtime.heap_mut())?;
        runtime.insert_named_resource("system", system);
        runtime
            .add_frame_system(Self::sys_track_visible_state.after(GpuStep::PresentTargetSurface));
        Ok(())
    }
}

#[inject_nitrous_resource]
impl System {
    pub fn new(heap: HeapMut) -> Result<Self> {
        let visible_widgets = Self::build_gui(heap)?;
        let system = Self {
            showing_normals: false,
            showing_help: false,
            visible_widgets,
        };
        Ok(system)
    }

    pub fn build_gui(mut heap: HeapMut) -> Result<VisibleWidgets> {
        let fnt = Fnt::from_bytes(
            "HUD11.FNT",
            heap.resource::<Libs>().read_name("HUD11.FNT")?.as_ref(),
        )?;
        let font = FntFont::from_fnt(&fnt)?;
        heap.resource_mut::<PaintContext>().add_font("HUD11", font);
        let _font_id = heap.resource::<PaintContext>().font_id_for_name("HUD11");

        let help = r#"How to use this program:
------------------------
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
        let mut help_box = LayoutNode::new_vbox("help_box", heap.as_mut())?;
        let mut help_line_ids = Vec::<Entity>::new();
        for (i, line) in help.lines().enumerate() {
            let help_line_id = Label::new(line)
                .with_font(heap.resource::<PaintContext>().font_id_for_name("mono"))
                .with_color(&Color::from([0, 255, 0]))
                .with_size(Size::from_pts(20.0))
                .wrapped(&format!("help_text_{}", i), heap.as_mut())?;
            heap.get_mut::<LayoutMeasurements>(help_line_id)
                .set_display(false);
            help_line_ids.push(help_line_id);
            help_box.push_widget(help_line_id)?;
        }
        let help_box_id = help_box.id();
        let help_packing = LayoutPacking::default()
            .float_middle()
            .float_center()
            .set_display(false)
            .set_background("#222a")?
            .set_padding_left("10px", heap.as_mut())?
            .set_padding_bottom("6px", heap.as_mut())?
            .set_padding_top("4px", heap.as_mut())?
            .set_padding_right("4px", heap.as_mut())?
            .set_border_color("#000")?
            .set_border_left("3px", heap.as_mut())?
            .set_border_right("3px", heap.as_mut())?
            .set_border_top("3px", heap.as_mut())?
            .set_border_bottom("3px", heap.as_mut())?
            .to_owned();
        *heap.get_mut::<LayoutPacking>(help_box_id) = help_packing;
        heap.resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_layout(help_box)?;

        let sim_time = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .with_font(heap.resource::<PaintContext>().font_id_for_name("mono"))
            .wrapped("sim_time", heap.as_mut())?;
        let fps_label = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .with_font(heap.resource::<PaintContext>().font_id_for_name("mono"))
            .wrapped("fps_label", heap.as_mut())?;
        let camera_fov = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .with_font(heap.resource::<PaintContext>().font_id_for_name("mono"))
            .wrapped("camera_fov", heap.as_mut())?;
        let mut controls_box = LayoutNode::new_vbox("controls_box", heap.as_mut())?;
        let controls_id = controls_box.id();
        controls_box.push_widget(sim_time)?;
        controls_box.push_widget(fps_label)?;
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

        fn make_label(name: &str, heap: HeapMut) -> Result<Entity> {
            Label::new("empty")
                .with_font(heap.resource::<PaintContext>().font_id_for_name("mono"))
                .with_color(&Color::from([0, 255, 0]))
                .with_size(Size::from_pts(12.0))
                .wrapped(name, heap)
        }
        let engine_label = make_label("engine_label", heap.as_mut())?;
        let airbrake_label = make_label("airbrake_label", heap.as_mut())?;
        let bay_label = make_label("bay_label", heap.as_mut())?;
        let flaps_label = make_label("flaps_label", heap.as_mut())?;
        let gear_label = make_label("gear_label", heap.as_mut())?;
        let hook_label = make_label("hook_label", heap.as_mut())?;

        let mut player_box = LayoutNode::new_vbox("player_box", heap.as_mut())?;
        let player_box_id = player_box.id();
        player_box.push_widget(engine_label)?;
        player_box.push_widget(airbrake_label)?;
        player_box.push_widget(bay_label)?;
        player_box.push_widget(flaps_label)?;
        player_box.push_widget(gear_label)?;
        player_box.push_widget(hook_label)?;
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
            sim_time,
            camera_fov,
            fps_label,

            engine_label,
            airbrake_label,
            bay_label,
            flaps_label,
            gear_label,
            hook_label,

            help_box_id,
            help_line_ids,
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
                &PowerSystem,
                &AirbrakeEffector,
                &BayEffector,
                &FlapsEffector,
                &GearEffector,
                &HookEffector,
            ),
            With<PlayerMarker>,
        >,
    ) {
        report!(system.track_visible_state(&camera, &timestep, &orrery, &mut labels, query));
    }

    fn track_visible_state(
        &self,
        camera: &ScreenCamera,
        timestep: &TimeStep,
        orrery: &Orrery,
        labels: &mut Query<&mut Label>,
        query: Query<
            (
                &PowerSystem,
                &AirbrakeEffector,
                &BayEffector,
                &FlapsEffector,
                &GearEffector,
                &HookEffector,
            ),
            With<PlayerMarker>,
        >,
    ) -> Result<()> {
        labels
            .get_mut(self.visible_widgets.sim_time)?
            .set_text(format!("Date: {:0.3}", orrery.get_time()));
        labels
            .get_mut(self.visible_widgets.camera_fov)?
            .set_text(format!("FoV: {}", degrees!(camera.fov_y())));
        labels
            .get_mut(self.visible_widgets.fps_label)?
            .set_text(format!(
                "Frame Time: {:>17.3}",
                timestep.sim_time().elapsed().as_secs_f64() * 1000.
            ));

        let (power, airbrake, bay, flaps, gear, hook) = query.single();
        labels
            .get_mut(self.visible_widgets.engine_label)?
            .set_text(format!("Engine:   {}", power.engine(0).current_power()));
        labels
            .get_mut(self.visible_widgets.airbrake_label)?
            .set_text(format!("Airbrake: {}", airbrake.position() > 0.));
        labels
            .get_mut(self.visible_widgets.bay_label)?
            .set_text(format!("Bay:      {}", bay.position() > 0.));
        labels
            .get_mut(self.visible_widgets.flaps_label)?
            .set_text(format!("Flaps:    {}", flaps.position() > 0.));
        labels
            .get_mut(self.visible_widgets.gear_label)?
            .set_text(format!("Gear:     {}", gear.position() > 0.));
        labels
            .get_mut(self.visible_widgets.hook_label)?
            .set_text(format!("Hook:     {}", hook.position() > 0.));

        Ok(())
    }

    #[method]
    fn toggle_show_help(&mut self, mut heap: HeapMut) -> Result<()> {
        self.showing_help = !self.showing_help;
        heap.get_mut::<LayoutPacking>(self.visible_widgets.help_box_id)
            .set_display(self.showing_help);
        for &line_id in &self.visible_widgets.help_line_ids {
            heap.get_mut::<LayoutMeasurements>(line_id)
                .set_display(self.showing_help);
        }
        Ok(())
    }

    #[method]
    fn toggle_show_normals(&mut self, mut heap: HeapMut) -> Result<()> {
        let id = heap.entity_by_name("Player");
        if self.showing_normals {
            heap.get_mut::<EntityMarkers>(id).clear_arrows();
            self.showing_normals = false;
            return Ok(());
        }
        self.showing_normals = true;

        let shape_id = *heap.get::<ShapeId>(id);
        let verts = heap
            .resource::<ShapeBuffer>()
            .read_back_vertices(shape_id, heap.resource::<Gpu>())?;
        for (i, vert) in verts.iter().enumerate() {
            heap.get_mut::<EntityMarkers>(id).add_arrow(
                &format!("n-{}", i),
                vert.point().map(|v| meters!(v)),
                vert.normal().map(|v| meters!(v * 10f32)),
                meters!(0.25_f64),
                "#F0F".parse()?,
            );
        }

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
        .insert_resource(opt.tracelog_opts)
        .insert_resource(opt.detail_opts.cpu_detail())
        .insert_resource(opt.detail_opts.gpu_detail())
        .insert_resource(app_dirs)
        .load_extension::<TraceLog>()?
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
        .load_extension::<ClassicFlightModel>()?
        .load_extension::<EnvelopeInstrument>()?
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
        .with_internal_tank(FuelTank::new(FuelTankKind::Center, kilograms!(0.)))?;
    let power = PowerSystem::default().with_engine(GliderEngine::default());
    let shape_ent = runtime
        .spawn_named("Player")?
        .insert(PlayerMarker)
        .insert(ShapeScale::new(1.))
        .insert_named(frame)?
        .insert_named(Airframe::new(kilograms!(10.)))?
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

    runtime.run_string(&*PRELUDE)?;
    runtime.run_startup();
    while runtime.resource::<ExitRequest>().still_running() {
        // Catch monotonic sim time up to system time.
        TimeStep::run_sim_loop(&mut runtime);

        // Display a frame
        runtime.run_frame_once();
    }

    Ok(())
}
