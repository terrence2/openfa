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
use absolute_unit::{feet, knots, meters, meters_per_second, miles_per_hour};
use anyhow::{bail, Result};
use bevy_ecs::prelude::*;
use csscolorparser::Color;
use flight_dynamics::ClassicFlightModel;
use gpu::Gpu;
use measure::{BodyMotion, WorldSpaceFrame};
use nitrous::{inject_nitrous_component, method, HeapMut, NitrousComponent};
use runtime::{report, Extension, PlayerMarker, Runtime};
use std::str::FromStr;
use triangulate::{builders, Triangulate, Vertex};
use widget::{
    Border, Extent, LayoutMeasurements, LayoutPacking, PaintContext, Position, Region, TextRun,
    WidgetInfo, WidgetRenderStep, WidgetVertex,
};
use window::{
    size::{RelSize, ScreenDir, Size},
    Window,
};
use xt::TypeRef;

#[derive(Copy, Clone, Debug, Default)]
struct PathVert {
    x: f32,
    y: f32,
}

impl Vertex for PathVert {
    type Coordinate = f32;

    fn x(&self) -> Self::Coordinate {
        self.x
    }

    fn y(&self) -> Self::Coordinate {
        self.y
    }
}

impl PathVert {
    pub fn to_widget(self, depth: f32, info: u32, color: [u8; 4]) -> WidgetVertex {
        WidgetVertex::new(
            [
                RelSize::Percent(self.x).as_gpu(),
                RelSize::Percent(self.y).as_gpu(),
                depth,
            ],
            [0., 0.],
            color,
            info,
        )
    }
}

// TODO: move this somewhere common
const INSTRUMENT_WIDTH: u32 = 81;
const INSTRUMENT_HEIGHT: u32 = 80;

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum EnvelopeRenderStep {
    Measure,
    Upload,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EnvelopeMode {
    Current,
    All,
    // Compare
}

impl FromStr for EnvelopeMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "current" => Self::Current,
            "all" => Self::All,
            _ => bail!("invalid envelope mode; must be 'current' or 'all'"),
        })
    }
}

#[derive(NitrousComponent, Debug)]
#[Name = "envelope"]
pub struct EnvelopeInstrument {
    scale: f32,
    mode: EnvelopeMode,
    max_g_load_output: TextRun,
    altitude_output: TextRun,
    velocity_output: TextRun,
}

impl Extension for EnvelopeInstrument {
    type Opts = ();
    fn init(runtime: &mut Runtime, _: ()) -> Result<()> {
        runtime.add_frame_system(
            EnvelopeInstrument::sys_measure
                .label(EnvelopeRenderStep::Measure)
                .before(WidgetRenderStep::LayoutWidgets),
        );
        runtime.add_frame_system(
            EnvelopeInstrument::sys_upload
                .label(EnvelopeRenderStep::Upload)
                .after(WidgetRenderStep::PrepareForFrame)
                .after(WidgetRenderStep::LayoutWidgets)
                .before(WidgetRenderStep::EnsureUploaded),
        );
        Ok(())
    }
}

#[inject_nitrous_component]
impl EnvelopeInstrument {
    const TMP_BACKGROUND_COLOR: [u8; 4] = [0x20, 0x30, 0x40, 0xff];
    const CURSOR_COLOR: [u8; 4] = [0xad, 0xe3, 0xfd, 0xff];
    const LEFT_COLOR: [u8; 4] = [0x4c, 0x8c, 0xa4, 0xff];
    const TOP_COLOR: [u8; 4] = [0xb0, 0xcc, 0xd8, 0xff];
    const BOTTOM_COLOR: [u8; 4] = [0x7c, 0xac, 0xbc, 0xff];
    const ENVELOPE_LAYER_COLORS: [[u8; 4]; 10] = [
        [0x20, 0x4c, 0x60, 0xff], // 0
        [0x2c, 0x60, 0x78, 0xff], // 1
        [0x3c, 0x74, 0x8c, 0xff],
        [0x4c, 0x8c, 0xa4, 0xff],
        [0x54, 0x90, 0xa8, 0xff],
        [0x5c, 0x98, 0xac, 0xff],
        [0x68, 0x9c, 0xb4, 0xff],
        [0x70, 0xa4, 0xb8, 0xff],
        [0x7c, 0xac, 0xbc, 0xff], // 8
        [0x84, 0xb0, 0xc0, 0xff], // 9
    ];

    fn sys_measure(
        player: Query<(&BodyMotion, &WorldSpaceFrame, &ClassicFlightModel), With<PlayerMarker>>,
        mut instruments: Query<(
            &mut EnvelopeInstrument,
            &LayoutPacking,
            &mut LayoutMeasurements,
        )>,
        win: Res<Window>,
        paint_context: Res<PaintContext>,
    ) {
        if let Ok((motion, frame, dynamics)) = player.get_single() {
            for (mut instrument, packing, mut measure) in instruments.iter_mut() {
                instrument.max_g_load_output.select_all();
                instrument
                    .max_g_load_output
                    .insert(&format!("{:0.1}", dynamics.max_g_load()));

                instrument.altitude_output.select_all();
                instrument.altitude_output.insert(&format!(
                    "{:0.0}",
                    feet!(frame.position_graticule().distance)
                ));

                instrument.velocity_output.select_all();
                instrument.velocity_output.insert(&format!(
                    "{:0.0}",
                    knots!(motion.vehicle_forward_velocity())
                ));

                report!(instrument
                    .altitude_output
                    .measure(&win, &paint_context.font_context));
                report!(instrument
                    .velocity_output
                    .measure(&win, &paint_context.font_context));
                report!(instrument
                    .max_g_load_output
                    .measure(&win, &paint_context.font_context));

                let extent = Extent::<RelSize>::new(
                    Size::from_px(INSTRUMENT_WIDTH as f32 * instrument.scale)
                        .as_rel(&win, ScreenDir::Horizontal),
                    Size::from_px(INSTRUMENT_HEIGHT as f32 * instrument.scale)
                        .as_rel(&win, ScreenDir::Vertical),
                );
                measure.set_child_extent(extent, packing);
            }
        }
    }

    fn sys_upload(
        player: Query<
            (&TypeRef, &BodyMotion, &WorldSpaceFrame, &ClassicFlightModel),
            With<PlayerMarker>,
        >,
        instruments: Query<(&EnvelopeInstrument, &mut LayoutMeasurements)>,
        win: Res<Window>,
        gpu: Res<Gpu>,
        mut paint_context: ResMut<PaintContext>,
    ) {
        if let Ok((xt, motion, frame, _dynamics)) = player.get_single() {
            for (instrument, measure) in instruments.iter() {
                let panel_border = Border::new(
                    Size::from_px(10.) * instrument.scale,
                    Size::from_px(12.) * instrument.scale,
                    Size::from_px(6.) * instrument.scale,
                    Size::from_px(6.) * instrument.scale,
                )
                .as_rel(&win);
                let info = paint_context.push_widget(&WidgetInfo::default());
                let mut region = measure.child_allocation().to_owned();
                // TODO: draw the panel image here instead of a gray square
                WidgetVertex::push_region(
                    region.clone(),
                    &Color::from(Self::TMP_BACKGROUND_COLOR),
                    info,
                    &mut paint_context.background_pool,
                );
                region.remove_border_rel(&panel_border);

                if let Some(pt) = xt.pt() {
                    // Generate poly and discover background color offsets
                    let display_width = meters_per_second!(miles_per_hour!(2000f32));
                    let display_height = meters!(feet!(95_000f32));
                    let mut max_x = 0f32;
                    let mut max_y = 0f32;
                    let mut x_of_max_y = 0f32;
                    let mut y_of_max_x = 0f32;
                    let mut polygons: Vec<(i16, Vec<Vec<PathVert>>)> = Vec::new();
                    for env in pt.envelopes.iter() {
                        // FIXME: get current gload from flight model
                        if instrument.mode == EnvelopeMode::All && env.gload >= 0
                            || instrument.mode == EnvelopeMode::Current && env.gload == 1
                        {
                            let mut polygon: Vec<Vec<PathVert>> = vec![vec![]];
                            for i in 0..env.count {
                                let coord = env.shape.coord(i as usize);

                                let xf = coord.speed().f32() / display_width.f32();
                                let yf = coord.altitude().f32() / display_height.f32();
                                let x = region.position().left().as_percent()
                                    + region.extent().width().as_percent() * xf;
                                let y = region.position().bottom().as_percent()
                                    + region.extent().height().as_percent() * yf;

                                if instrument.mode == EnvelopeMode::All && env.gload == 1
                                    || instrument.mode == EnvelopeMode::Current
                                {
                                    if y > max_y {
                                        max_y = y;
                                        x_of_max_y = x;
                                    }
                                    if x > max_x {
                                        max_x = x;
                                        y_of_max_x = y;
                                    }
                                }

                                polygon[0].push(PathVert { x, y });
                            }
                            polygons.push((env.gload, polygon));
                        }
                    }

                    // Paint background
                    let extent_left = RelSize::Percent(x_of_max_y) - region.position().left();
                    let extent_bottom = RelSize::Percent(y_of_max_x) - region.position().bottom();
                    WidgetVertex::push_region(
                        Region::new(
                            region.position().clone_with_depth_adjust(0.1),
                            Extent::new(extent_left, region.extent().height()),
                        ),
                        &Color::from(Self::LEFT_COLOR),
                        info,
                        &mut paint_context.background_pool,
                    );
                    WidgetVertex::push_region(
                        Region::new(
                            Position::new_with_depth(
                                RelSize::Percent(x_of_max_y),
                                region.position().bottom(),
                                region.position().depth() + RelSize::Gpu(0.1),
                            ),
                            Extent::new(region.extent().width() - extent_left, extent_bottom),
                        ),
                        &Color::from(Self::BOTTOM_COLOR),
                        info,
                        &mut paint_context.background_pool,
                    );
                    WidgetVertex::push_region(
                        Region::new(
                            Position::new_with_depth(
                                RelSize::Percent(x_of_max_y),
                                region.position().bottom() + extent_bottom,
                                region.position().depth() + RelSize::Gpu(0.1),
                            ),
                            Extent::new(
                                region.extent().width() - extent_left,
                                region.extent().height() - extent_bottom,
                            ),
                        ),
                        &Color::from(Self::TOP_COLOR),
                        info,
                        &mut paint_context.background_pool,
                    );

                    // Triangulate and paint envelope(s)
                    for (i, (gload, polygon)) in polygons.iter().enumerate() {
                        let mut out: Vec<Vec<PathVert>> = vec![];
                        polygon
                            .triangulate::<builders::VecVecFanBuilder<_>>(&mut out)
                            .unwrap();
                        let depth = region.position().depth().as_gpu() + 0.2 + 0.01 * i as f32;
                        let color = Self::ENVELOPE_LAYER_COLORS[(gload.unsigned_abs() as usize)
                            .min(Self::ENVELOPE_LAYER_COLORS.len() - 1)];
                        for tri in &out {
                            // For each tri in the fan
                            let v0 = &tri[0];
                            for i in 0..tri.len() - 2 {
                                paint_context
                                    .background_pool
                                    .push(v0.to_widget(depth, info, color));
                                paint_context
                                    .background_pool
                                    .push(tri[i + 1].to_widget(depth, info, color));
                                paint_context
                                    .background_pool
                                    .push(tri[i + 2].to_widget(depth, info, color));
                            }
                        }
                    }

                    // Paint cursor
                    let xf = motion.vehicle_forward_velocity().f32() / display_width.f32();
                    let yf = frame.altitude_asl().f32() / display_height.f32();
                    let x = region.position().left().as_percent()
                        + region.extent().width().as_percent() * xf;
                    let y = region.position().bottom().as_percent()
                        + region.extent().height().as_percent() * yf;
                    WidgetVertex::push_region(
                        Region::new(
                            Position::new_with_depth(
                                RelSize::Percent(x)
                                    - (Size::from_px(1.) * instrument.scale)
                                        .as_rel(&win, ScreenDir::Horizontal),
                                RelSize::Percent(y)
                                    - (Size::from_px(1.) * instrument.scale)
                                        .as_rel(&win, ScreenDir::Vertical),
                                region.position().depth() + RelSize::Gpu(0.3),
                            ),
                            Extent::new(
                                (Size::from_px(2.) * instrument.scale)
                                    .as_rel(&win, ScreenDir::Horizontal),
                                (Size::from_px(2.) * instrument.scale)
                                    .as_rel(&win, ScreenDir::Vertical),
                            ),
                        ),
                        &Color::from(Self::CURSOR_COLOR),
                        info,
                        &mut paint_context.background_pool,
                    );

                    // Draw text on top
                    let mut pos = region.position().clone_with_depth_adjust(0.4);
                    *pos.left_mut() += region.extent().width()
                        - instrument
                            .max_g_load_output
                            .metrics()
                            .width
                            .as_rel(&win, ScreenDir::Horizontal);
                    *pos.bottom_mut() += region.extent().height()
                        - instrument
                            .max_g_load_output
                            .metrics()
                            .height
                            .as_rel(&win, ScreenDir::Vertical);
                    report!(instrument.max_g_load_output.upload(
                        pos.into(),
                        info,
                        &win,
                        &gpu,
                        &mut paint_context
                    ));

                    let mut pos = region.position().clone_with_depth_adjust(0.4);
                    *pos.bottom_mut() += region.extent().height()
                        - instrument
                            .altitude_output
                            .metrics()
                            .height
                            .as_rel(&win, ScreenDir::Vertical);
                    report!(instrument.altitude_output.upload(
                        pos.into(),
                        info,
                        &win,
                        &gpu,
                        &mut paint_context
                    ));

                    let mut pos = region.position().clone_with_depth_adjust(0.4);
                    *pos.left_mut() += region.extent().width()
                        - instrument
                            .velocity_output
                            .metrics()
                            .width
                            .as_rel(&win, ScreenDir::Horizontal);
                    report!(instrument.velocity_output.upload(
                        pos.into(),
                        info,
                        &win,
                        &gpu,
                        &mut paint_context
                    ));
                }
            }
        }
        // println!("ENV: {:?}", now.elapsed());
    }

    pub fn new(context: &PaintContext) -> Self {
        let scale = 4.;
        let font_id = context.font_context.font_id_for_name("HUD11");
        let font_size = Size::from_pts(6. * scale);
        Self {
            scale,
            mode: EnvelopeMode::Current,
            max_g_load_output: TextRun::empty()
                .with_hidden_selection()
                .with_default_color(&Color::from([1., 1., 1.]))
                .with_default_font(font_id)
                .with_default_size(font_size),
            altitude_output: TextRun::empty()
                .with_hidden_selection()
                .with_default_color(&Color::from([1., 1., 1.]))
                .with_default_font(font_id)
                .with_default_size(font_size),
            velocity_output: TextRun::empty()
                .with_hidden_selection()
                .with_default_color(&Color::from([1., 1., 1.]))
                .with_default_font(font_id)
                .with_default_size(font_size),
        }
    }

    #[method]
    pub fn set_mode(&mut self, mode: &str) -> Result<&mut Self> {
        self.mode = EnvelopeMode::from_str(mode)?;
        Ok(self)
    }

    #[method]
    pub fn set_scale(&mut self, scale: f64) -> &mut Self {
        let scale = scale as f32;
        self.scale = scale;
        let font_size = Size::from_pts(6. * scale);
        self.max_g_load_output.set_default_size(font_size);
        self.altitude_output.set_default_size(font_size);
        self.velocity_output.set_default_size(font_size);
        self.max_g_load_output.select_all();
        self.altitude_output.select_all();
        self.velocity_output.select_all();
        self.max_g_load_output.change_size(font_size);
        self.altitude_output.change_size(font_size);
        self.velocity_output.change_size(font_size);
        self
    }

    pub fn wrapped(self, name: &str, mut heap: HeapMut) -> Result<Entity> {
        Ok(heap
            .spawn_named(name)?
            .insert_named(self)?
            .insert_named(LayoutPacking::default())?
            .insert(LayoutMeasurements::default())
            .id())
    }
}
