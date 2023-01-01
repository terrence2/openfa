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
use anyhow::{bail, Result};
use egui::{
    epaint::{Vertex as EVertex, WHITE_UV},
    Align2, Color32, FontId, Mesh, Pos2, Rounding, TextureId, Vec2,
};
use flight_dynamics::ClassicFlightModel;
use measure::{BodyMotion, WorldSpaceFrame};
use std::str::FromStr;
use triangulate::{builders, Triangulate, Vertex};
use xt::TypeRef;

// TODO: move this somewhere common
const INSTRUMENT_WIDTH: u32 = 81;
const INSTRUMENT_HEIGHT: u32 = 80;

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

pub struct EnvelopeInstrument {
    scale: f32,
    mode: EnvelopeMode,
}

impl Default for EnvelopeInstrument {
    fn default() -> Self {
        Self {
            scale: 4.,
            mode: EnvelopeMode::All,
        }
    }
}

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

    fn clr(arr: [u8; 4]) -> Color32 {
        Color32::from_rgba_premultiplied(arr[0], arr[1], arr[2], arr[3])
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale as f32;
    }

    pub fn set_mode(&mut self, mode: &str) -> Result<&mut Self> {
        self.mode = EnvelopeMode::from_str(mode)?;
        Ok(self)
    }

    pub fn display_width(&self) -> f32 {
        INSTRUMENT_WIDTH as f32 * self.scale
    }

    pub fn display_height(&self) -> f32 {
        INSTRUMENT_HEIGHT as f32 * self.scale
    }

    pub fn ui(
        &self,
        ui: &mut egui::Ui,
        xt: &TypeRef,
        motion: &BodyMotion,
        frame: &WorldSpaceFrame,
        flight: &ClassicFlightModel,
    ) {
        // Compute various extents
        let extent = Vec2::new(self.display_width(), self.display_height());
        let (_response, painter) = ui.allocate_painter(extent, egui::Sense::hover());
        let clip = painter.clip_rect();
        let mut screen = clip;
        screen.min.x += 6. * self.scale;
        screen.max.x -= 6. * self.scale;
        screen.min.y += 10. * self.scale;
        screen.max.y -= 12. * self.scale;

        // Convert our representation into what the triangulator needs and compute
        // various min/max locations so we can paint the background.
        if let Some(pt) = xt.pt() {
            // Generate poly and discover background color offsets
            let display_width = meters_per_second!(miles_per_hour!(2000f32));
            let display_height = meters!(feet!(95_000f32));
            let mut max_x = 0f32;
            let mut min_y = f32::INFINITY;
            let mut x_of_max_y = 0f32;
            let mut y_of_max_x = 0f32;
            let mut polygons: Vec<(i16, Vec<Vec<PathVert>>)> = Vec::new();
            for env in pt.envelopes.iter() {
                // FIXME: get current gload from flight model
                if self.mode == EnvelopeMode::All && env.gload >= 0
                    || self.mode == EnvelopeMode::Current && env.gload == 1
                {
                    let mut polygon: Vec<Vec<PathVert>> = vec![vec![]];
                    for i in 0..env.count {
                        let coord = env.shape.coord(i as usize);

                        let xf = coord.speed().f32() / display_width.f32();
                        let yf = coord.altitude().f32() / display_height.f32();
                        let x = screen.min.x + screen.width() * xf;
                        let y = screen.max.y - screen.height() * yf;

                        if self.mode == EnvelopeMode::All && env.gload == 1
                            || self.mode == EnvelopeMode::Current
                        {
                            if y < min_y {
                                min_y = y;
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

            // Draw the background
            painter.rect_filled(
                clip,
                Rounding::none(),
                Self::clr(Self::TMP_BACKGROUND_COLOR),
            );
            let mut left = screen;
            left.max.x = x_of_max_y;
            painter.rect_filled(left, Rounding::none(), Self::clr(Self::LEFT_COLOR));
            let mut bottom = screen;
            bottom.min.x = left.max.x;
            bottom.min.y = y_of_max_x;
            painter.rect_filled(bottom, Rounding::none(), Self::clr(Self::BOTTOM_COLOR));
            let mut top = screen;
            top.min.x = left.max.x;
            top.max.y = bottom.min.y;
            painter.rect_filled(top, Rounding::none(), Self::clr(Self::TOP_COLOR));

            // Triangulate and paint envelope(s)
            for (gload, polygon) in polygons.iter() {
                let mut mesh = Mesh {
                    texture_id: TextureId::Managed(0),
                    ..Default::default()
                };
                let mut tris: Vec<Vec<PathVert>> = vec![];
                polygon
                    .triangulate::<builders::VecVecFanBuilder<_>>(&mut tris)
                    .unwrap();

                let color = Self::clr(
                    Self::ENVELOPE_LAYER_COLORS[(gload.unsigned_abs() as usize)
                        .min(Self::ENVELOPE_LAYER_COLORS.len() - 1)],
                );
                for tri in &tris {
                    // For each tri in the fan
                    let v0 = &tri[0];
                    for i in 0..tri.len() - 2 {
                        mesh.vertices.push(EVertex {
                            pos: [v0.x, v0.y].into(),
                            uv: WHITE_UV,
                            color,
                        });
                        mesh.indices.push(mesh.indices.len() as u32);
                        mesh.vertices.push(EVertex {
                            pos: [tri[i + 1].x, tri[i + 1].y].into(),
                            uv: WHITE_UV,
                            color,
                        });
                        mesh.indices.push(mesh.indices.len() as u32);
                        mesh.vertices.push(EVertex {
                            pos: [tri[i + 2].x, tri[i + 2].y].into(),
                            uv: WHITE_UV,
                            color,
                        });
                        mesh.indices.push(mesh.indices.len() as u32);
                    }
                }

                painter.add(mesh);
            }

            // Paint cursor
            let xf = motion.vehicle_forward_velocity().f32() / display_width.f32();
            let yf = frame.altitude_asl().f32() / display_height.f32();
            painter.circle_filled(
                Pos2::new(
                    screen.left() + screen.width() * xf,
                    screen.bottom() - screen.height() * yf,
                ),
                (self.scale / 1.5).max(1.),
                Self::clr(Self::CURSOR_COLOR),
            );

            painter.text(
                screen.right_top(),
                Align2::RIGHT_TOP,
                format!("{}", flight.max_g_load()),
                FontId::monospace(6. * self.scale),
                Color32::DARK_GRAY,
            );
            painter.text(
                screen.left_top(),
                Align2::LEFT_TOP,
                format!("{:0.0}", feet!(frame.altitude_asl())),
                FontId::monospace(6. * self.scale),
                Color32::WHITE,
            );
            painter.text(
                screen.right_bottom(),
                Align2::RIGHT_BOTTOM,
                format!("{:0.0}", knots!(motion.vehicle_forward_velocity())),
                FontId::monospace(6. * self.scale),
                Color32::WHITE,
            );
        }
    }
}
