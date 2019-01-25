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
use asset::AssetManager;
use failure::{bail, ensure, err_msg, Fallible};
use image::{ImageBuffer, Rgba};
use lay::Layer;
use lib::Library;
use log::trace;
use mm::{MapOrientation, MissionMap, TLoc};
use nalgebra::{Isometry3, Matrix4};
use omnilib::OmniLib;
use pal::Palette;
use pic::decode_pic;
use render::{ArcBallCamera, T2Renderer};
use simplelog::{Config, LevelFilter, TermLogger};
use std::{collections::HashMap, sync::Arc, time::Instant};
use structopt::StructOpt;
use t2::Terrain;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{AutoCommandBufferBuilder, DynamicState},
    descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet},
    device::Device,
    format::Format,
    framebuffer::Subpass,
    image::{Dimensions, ImmutableImage},
    impl_vertex,
    pipeline::{GraphicsPipeline, GraphicsPipelineAbstract},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use window::{GraphicsConfigBuilder, GraphicsWindow};
use winit::{
    DeviceEvent::{Button, Key, MouseMotion, MouseWheel},
    ElementState,
    Event::{DeviceEvent, WindowEvent},
    KeyboardInput, MouseScrollDelta, VirtualKeyCode,
    WindowEvent::{CloseRequested, Destroyed, Resized},
};
use xt::TypeManager;

/*
fn get_files(input: &str) -> Vec<PathBuf> {
    let path = Path::new(input);
    if path.is_dir() {
        return path
            .read_dir()
            .unwrap()
            .map(|p| p.unwrap().path().to_owned())
            .collect::<Vec<_>>();
    }
    return vec![path.to_owned()];
}
*/

// These are all of the terrains and map references in the base games.
// FA:
//     FA_2.LIB:
//         EGY.T2, FRA.T2, VLA.T2, BAL.T2, UKR.T2, KURILE.T2, TVIET.T2
//         APA.T2, CUB.T2, GRE.T2, IRA.T2, LFA.T2, NSK.T2, PGU.T2, SPA.T2, WTA.T2
//     MM refs:
//         // Campaign missions?
//         $bal[0-7].T2
//         $egy[1-9].T2
//         $fra[0-9].T2
//         $vla[1-8].T2
//         ~ukr[1-8].T2
//         // Freeform missions and ???; map editor layouts maybe?
//         ~apaf.T2, apa.T2
//         ~balf.T2, bal.T2
//         ~cubf.T2, cub.T2
//         ~egyf.T2, egy.T2
//         ~fraf.T2, fra.T2
//         ~gref.T2, gre.T2
//         ~iraf.T2, ira.T2
//         ~kurile.T2, kurile.T2
//         ~lfaf.T2, lfa.T2
//         ~nskf.T2, nsk.T2
//         ~pguf.T2, pgu.T2
//         ~spaf.T2, spa.T2
//         ~tviet.T2, tviet.T2
//         ~ukrf.T2, ukr.T2
//         ~vlaf.T2, vla.T2
//         ~wtaf.T2, wta.T2
//    M refs:
//         $bal[0-7].T2
//         $egy[1-8].T2
//         $fra[0-3,6-9].T2
//         $vla[1-8].T2
//         ~bal[0,2,3,6,7].T2
//         ~egy[1,2,4,7].T2
//         ~fra[3,9].T2
//         ~ukr[1-8].T2
//         ~vla[1,2,5].T2
//         bal.T2, cub.T2, egy.T2, fra.T2, kurile.T2, tviet.T2, ukr.T2, vla.T2
// USNF97:
//     USNF_2.LIB: UKR.T2, ~UKR[1-8].T2, KURILE.T2, VIET.T2
//     MM refs: ukr.T2, ~ukr[1-8].T2, kurile.T2, viet.T2
//     M  refs: ukr.T2, ~ukr[1-8].T2, kurile.T2, viet.T2
// ATFGOLD:
//     ATF_2.LIB: EGY.T2, FRA.T2, VLA.T2, BAL.T2
//     MM refs: egy.T2, fra.T2, vla.T2, bal.T2
//              $egy[1-9].T2, $fra[0-9].T2, $vla[1-8].T2, $bal[0-7].T2
//     INVALID: kurile.T2, ~ukr[1-8].T2, ukr.T2, viet.T2
//     M  refs: $egy[1-8].T2, $fra[0-3,6-9].T2, $vla[1-8].T2, $bal[0-7].T2,
//              ~bal[2,6].T2, bal.T2, ~egy4.T2, egy.T2, fra.T2, vla.T2
//     INVALID: ukr.T2
// ATFNATO:
//     installdir: EGY.T2, FRA.T2, VLA.T2, BAL.T2
//     MM refs: egy.T2, fra.T2, vla.T2, bal.T2,
//              $egy[1-9].T2, $fra[0-9].T2, $vla[1-8].T2, $bal[0-7].T2
//     M  refs: egy.T2, fra.T2, vla.T2, bal.T2,
//              $egy[1-8].T2, $fra[0-3,6-9].T2, $vla[1-8].T2, $bal[0-7].T2
// ATF:
//     installdir: EGY.T2, FRA.T2, VLA.T2
//     MM refs: egy.T2, fra.T2, vla.T2,
//              $egy[1-8].T2, $fra[0-9].T2, $vla[1-8].T2
//     M  refs: $egy[1-8].T2, $fra[0-3,6-9].T2, $vla[1-8].T2, egy.T2
// MF:
//     installdir: UKR.T2, $UKR[1-8].T2, KURILE.T2
//     MM+M refs: ukr.T2, $ukr[1-8].T2, kurile.T2
// USNF:
//     installdir: UKR.T2, $UKR[1-8].T2
//     MM+M refs: ukr.T2, $ukr[1-8].T2
pub fn load_t2_for_map(
    raw: &str,
    assets: &Arc<Box<AssetManager>>,
    lib: &Arc<Box<Library>>,
) -> Fallible<Arc<Box<Terrain>>> {
    if lib.file_exists(raw) {
        return assets.load_t2(raw);
    }

    // ~KURILE.T2 && ~TVIET.T2
    if raw.starts_with('~') && lib.file_exists(&raw[1..]) {
        return assets.load_t2(&raw[1..]);
    }

    let parts = raw.split('.').collect::<Vec<&str>>();
    let base = parts[0];
    if base.len() == 5 {
        let sigil = base.chars().next().unwrap();
        ensure!(
            sigil == '~' || sigil == '$',
            "expected non-literal map name to start with $ or ~"
        );
        let suffix = base.chars().rev().take(1).collect::<String>();
        ensure!(
            suffix == "F" || suffix.parse::<u8>().is_ok(),
            "expected non-literal map name to end with f or a number"
        );
        return assets.load_t2(&(base[1..=3].to_owned() + ".T2"));
    }

    bail!("no map file matching {} found", raw)
}


struct PaletteOverlayRenderer {
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "mm_explorer", about = "Show the contents of an mm file")]
struct Opt {
    #[structopt(
        short = "g",
        long = "game",
        default_value = "FA",
        help = "The game libraries to load."
    )]
    game: String,

    #[structopt(help = "Will load it from game, or look at last component of path")]
    input: String,
}

pub fn main() -> Fallible<()> {
    let opt = Opt::from_args();
    TermLogger::init(LevelFilter::Trace, Config::default())?;

    let omnilib = OmniLib::new_for_test()?;
    let lib = omnilib.library(&opt.game);

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    let assets = Arc::new(Box::new(AssetManager::new(lib.clone())?));
    let types = TypeManager::new(lib.clone())?;

    let contents = lib.load_text(&opt.input)?;
    let mm = MissionMap::from_str(&contents, &types)?;

    ///////////////////////////////////////////////////////////
    let mut t2_renderer = T2Renderer::new(mm, &assets, &lib, &window)?;
    let mut e0_off = 0;
    let mut f1_off = 0;
    let mut c2_off = 0;
    let mut d3_off = 0;
    t2_renderer.set_palette_parameters(&window, e0_off, f1_off, c2_off, d3_off)?;
    ///////////////////////////////////////////////////////////

    let model = Isometry3::new(nalgebra::zero(), nalgebra::zero());
    let mut camera = ArcBallCamera::new(window.aspect_ratio()?);

    let mut need_reset = false;
    loop {
        let loop_start = Instant::now();

        if need_reset == true {
            need_reset = false;
            t2_renderer.set_palette_parameters(&window, e0_off, f1_off, c2_off, d3_off)?;
        }

        t2_renderer.set_projection(camera.projection_for(model));

        window.drive_frame(|command_buffer, dynamic_state| {
            t2_renderer.render(command_buffer, dynamic_state)
        })?;

        let mut done = false;
        let mut resized = false;
        window.events_loop.poll_events(|ev| match ev {
            WindowEvent {
                event: CloseRequested,
                ..
            } => done = true,
            WindowEvent {
                event: Destroyed, ..
            } => done = true,
            WindowEvent {
                event: Resized(_), ..
            } => resized = true,

            // Mouse motion
            DeviceEvent {
                event: MouseMotion { delta: (x, y) },
                ..
            } => {
                camera.on_mousemove(x as f32, y as f32);
            }
            DeviceEvent {
                event:
                    MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                    },
                ..
            } => camera.on_mousescroll(x, y),
            DeviceEvent {
                event:
                    Button {
                        button: id,
                        state: ElementState::Pressed,
                    },
                ..
            } => camera.on_mousebutton_down(id),
            DeviceEvent {
                event:
                    Button {
                        button: id,
                        state: ElementState::Released,
                    },
                ..
            } => camera.on_mousebutton_up(id),

            // Keyboard Press
            DeviceEvent {
                event:
                    Key(KeyboardInput {
                        virtual_keycode: Some(keycode),
                        state: ElementState::Pressed,
                        ..
                    }),
                ..
            } => match keycode {
                VirtualKeyCode::Escape => done = true,
                VirtualKeyCode::R => need_reset = true,
                VirtualKeyCode::Y => e0_off += 1,
                VirtualKeyCode::H => e0_off -= 1,
                VirtualKeyCode::U => f1_off += 1,
                VirtualKeyCode::J => f1_off -= 1,
                VirtualKeyCode::I => c2_off += 1,
                VirtualKeyCode::K => c2_off -= 1,
                VirtualKeyCode::O => d3_off += 1,
                VirtualKeyCode::L => d3_off -= 1,
                _ => trace!("unknown keycode: {:?}", keycode),
            },

            _ => (),
        });
        if done {
            return Ok(());
        }
        if resized {
            window.note_resize()
        }

        let offsets = format!("e0:{} f1:{} c2:{} d3:{}", e0_off, f1_off, c2_off, d3_off);
        window.debug_text(1800f32, 25f32, 30f32, [1f32, 0f32, 0f32, 1f32], &offsets);

        let ft = loop_start.elapsed();
        let ts = format!(
            "{}.{} ms",
            ft.as_secs() * 1000 + ft.subsec_millis() as u64,
            ft.subsec_micros()
        );
        window.debug_text(10f32, 30f32, 15f32, [1f32, 1f32, 1f32, 1f32], &ts);
    }
}
