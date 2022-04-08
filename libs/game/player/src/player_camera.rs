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
use camera::{ScreenCamera, ScreenCameraController};
use geodesy::{Cartesian, GeoCenter};
use global_data::GlobalsStep;
use measure::WorldSpaceFrame;
use nalgebra::{UnitQuaternion, Vector3};
use nitrous::{
    inject_nitrous_component, inject_nitrous_resource, method, HeapMut, NitrousComponent,
    NitrousResource,
};
use runtime::{Extension, Runtime};
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

#[derive(Component, NitrousComponent, Debug)]
#[Name = "controller"]
pub struct PlayerCameraController {
    mode: CameraMode,
}

impl Extension for PlayerCameraController {
    fn init(runtime: &mut Runtime) -> Result<()> {
        // let player = Self::default();
        // runtime.insert_named_resource("camera", player);
        // runtime.run_string(
        //     r#"
        //         bindings.bind("+mouse1", "@camera.arcball.pan_view(pressed)");
        //         bindings.bind("+mouse3", "@camera.arcball.move_view(pressed)");
        //         bindings.bind("mouseMotion", "@camera.arcball.handle_mousemotion(dx, dy)");
        //         bindings.bind("mouseWheel", "@camera.arcball.handle_mousewheel(vertical_delta)");
        //         bindings.bind("+Shift+Up", "@camera.arcball.target_up_fast(pressed)");
        //         bindings.bind("+Shift+Down", "@camera.arcball.target_down_fast(pressed)");
        //         bindings.bind("+Up", "@camera.arcball.target_up(pressed)");
        //         bindings.bind("+Down", "@camera.arcball.target_down(pressed)");
        //     "#,
        // )?;
        runtime.run_string(
            r#"
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
            "#,
        )?;

        runtime
            .spawn_named("camera")?
            .insert_named(PlayerCameraController::new())?
            // bind this frame to the screen
            .insert_named(WorldSpaceFrame::default())?
            .insert(ScreenCameraController::default())
            .id();

        runtime.add_frame_system(
            Self::sys_update_camera
                .before(GlobalsStep::TrackStateChanges)
                .before(TerrainStep::OptimizePatches),
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
