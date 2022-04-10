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
use crate::cameras::ExternalCameraController;
use absolute_unit::Meters;
use anyhow::{bail, Result};
use asset_loader::PlayerMarker;
use bevy_ecs::prelude::*;
use camera::{CameraStep, ScreenCamera, ScreenCameraController};
use flight_dynamics::FlightStep;
use geodesy::{Cartesian, GeoCenter};
use global_data::GlobalsStep;
use measure::WorldSpaceFrame;
use nalgebra::{UnitQuaternion, Vector3};
use nitrous::{
    inject_nitrous_component, inject_nitrous_resource, method, HeapMut, NitrousComponent,
    NitrousResource,
};
use runtime::{Extension, Runtime};
use shape::ShapeStep;
use std::str::FromStr;
use terrain::TerrainStep;

#[derive(Debug)]
pub enum CameraMode {
    Forward,
    Backward,
    LookUp,
    TrackTarget,
    PlayerToIncoming,
    PlayerToWingman,
    PlayerToTarget,
    TargetToPlayer,
    FlyBy,
    External(ExternalCameraController),
    MissleToTarget,
}

impl FromStr for CameraMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::prelude::rust_2015::Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "forward" => Self::Forward,
            "backward" => Self::Backward,
            "up" | "lookup" => Self::LookUp,
            "target" | "tracktarget" => Self::TrackTarget,
            "incoming" | "playertoincoming" => Self::PlayerToIncoming,
            "wingman" | "playertowingman" => Self::PlayerToWingman,
            "playertotarget" => Self::PlayerToTarget,
            "targettoplayer" => Self::TargetToPlayer,
            "flyby" => Self::FlyBy,
            "external" => Self::External(ExternalCameraController::default()),
            "missle" | "missletotarget" => Self::MissleToTarget,
            _ => bail!("unknown camera mode"),
        })
    }
}

impl CameraMode {
    fn set(&mut self, name: &str) -> Result<()> {
        *self = CameraMode::from_str(name)?;
        Ok(())
    }

    fn set_pan_view(&mut self, pressed: bool) {
        match self {
            Self::External(controller) => controller.set_pan_view(pressed),
            _ => {}
        }
    }

    fn handle_mousemotion(&mut self, dx: f64, dy: f64) {
        match self {
            Self::External(controller) => controller.handle_mousemotion(dx, dy),
            _ => {}
        }
    }

    fn handle_mousewheel(&mut self, vertical_delta: f64) {
        match self {
            Self::External(controller) => controller.handle_mousewheel(vertical_delta),
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, SystemLabel)]
pub enum PlayerCameraStep {
    ApplyInput,
}

#[derive(Component, NitrousComponent, Debug)]
#[Name = "controller"]
pub struct PlayerCameraController {
    mode: CameraMode,
}

impl Extension for PlayerCameraController {
    fn init(runtime: &mut Runtime) -> Result<()> {
        runtime
            .spawn_named("camera")?
            .insert_named(PlayerCameraController::new())?
            // bind this frame to the screen
            .insert_named(WorldSpaceFrame::default())?
            .insert(ScreenCameraController::default())
            .id();

        runtime.add_sim_system(
            Self::sys_update_camera
                .label(PlayerCameraStep::ApplyInput)
                .before(CameraStep::ApplyInput),
        );

        Ok(())
    }
}

#[inject_nitrous_component]
impl PlayerCameraController {
    fn new() -> Self {
        Self {
            mode: CameraMode::Forward,
        }
    }

    fn sys_update_camera(
        player_query: Query<(&WorldSpaceFrame, &PlayerMarker)>,
        mut camera_query: Query<
            (&mut WorldSpaceFrame, &PlayerCameraController),
            Without<PlayerMarker>,
        >,
    ) {
        if let Ok((mut camera_frame, camera)) = camera_query.get_single_mut() {
            if let Ok((player_frame, _marker)) = player_query.get_single() {
                // TODO - target, missile, etc
                match &camera.mode {
                    CameraMode::Forward => {
                        *camera_frame = player_frame.to_owned();
                    }
                    CameraMode::External(controller) => {
                        *camera_frame = controller.get_frame(player_frame);
                    }
                    _ => {
                        println!("skipping cameras update for unimplemented mode")
                    }
                }
            }
        }

        // let facing = camera_frame.facing();
        // *camera_frame.facing_mut() = facing;
    }

    #[method]
    fn set_mode(&mut self, name: &str) -> Result<()> {
        self.mode.set(name)
    }

    #[method]
    fn set_pan_view(&mut self, pressed: bool) {
        self.mode.set_pan_view(pressed);
    }

    #[method]
    fn handle_mousemotion(&mut self, dx: f64, dy: f64) {
        self.mode.handle_mousemotion(dx, dy);
    }

    #[method]
    fn handle_mousewheel(&mut self, vertical_delta: f64) {
        self.mode.handle_mousewheel(vertical_delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
