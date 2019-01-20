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
mod texture_atlas;

use crate::texture_atlas::TextureAtlas;
use asset::AssetManager;
use failure::{bail, ensure, err_msg, Fallible};
use image::{ImageBuffer, Rgba};
use lay::Layer;
use lib::Library;
use log::trace;
use mm::{MissionMap, TLoc, MapOrientation};
use nalgebra::{Isometry3, Matrix4};
use omnilib::OmniLib;
use pal::Palette;
use pic::decode_pic;
use render::ArcBallCamera;
use simplelog::{Config, LevelFilter, TermLogger};
use std::{collections::HashMap, sync::Arc};
use structopt::StructOpt;
use t2::Terrain;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
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

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    tex_coord: [f32; 2],
}

impl_vertex!(Vertex, position, color, tex_coord);

mod vs {
    use vulkano_shaders::shader;

    shader! {
    ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec3 position;
            layout(location = 1) in vec4 color;
            layout(location = 2) in vec2 tex_coord;

            layout(push_constant) uniform PushConstantData {
              mat4 projection;
            } pc;

            layout(location = 0) out vec4 v_color;
            layout(location = 1) out vec2 v_tex_coord;

            void main() {
                gl_Position = pc.projection * vec4(position, 1.0);
                v_color = color;
                v_tex_coord = tex_coord;
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
    ty: "fragment",
        src: "
            #version 450

            layout(location = 0) in vec4 v_color;
            layout(location = 1) in vec2 v_tex_coord;

            layout(location = 0) out vec4 f_color;

            layout(set = 0, binding = 0) uniform sampler2D tex;

float sRGB(float x) {
    if (x <= 0.00031308)
        return 12.92 * x;
    else
        return 1.055*pow(x,(1.0 / 2.4) ) - 0.055;
}

vec4 sRGB_v3(vec4 c) {
    return vec4(sRGB(c.r),sRGB(c.g),sRGB(c.b),sRGB(c.a));
}

            void main() {
                if (v_tex_coord.x == 0.0) {
                    f_color = v_color;
                } else {
                    f_color = sRGB_v3(texture(tex, v_tex_coord));
                }
            }
            "
    }
}

impl vs::ty::PushConstantData {
    fn new() -> Self {
        Self {
            projection: [
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                [0.0f32, 0.0f32, 0.0f32, 0.0f32],
            ],
        }
    }

    fn set_projection(&mut self, mat: Matrix4<f32>) {
        self.projection[0][0] = mat[0];
        self.projection[0][1] = mat[1];
        self.projection[0][2] = mat[2];
        self.projection[0][3] = mat[3];
        self.projection[1][0] = mat[4];
        self.projection[1][1] = mat[5];
        self.projection[1][2] = mat[6];
        self.projection[1][3] = mat[7];
        self.projection[2][0] = mat[8];
        self.projection[2][1] = mat[9];
        self.projection[2][2] = mat[10];
        self.projection[2][3] = mat[11];
        self.projection[3][0] = mat[12];
        self.projection[3][1] = mat[13];
        self.projection[3][2] = mat[14];
        self.projection[3][3] = mat[15];
    }
}

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
    let sym = parts[0];
    if sym.len() == 5 {
        let ss = sym.chars().next().unwrap();
        let se = sym.chars().rev().take(1).collect::<String>();
        println!("SYM: {}, ss: {}, se: {}", sym, ss, se);
        ensure!(
            ss == '~' || ss == '$',
            "expected non-literal map name to start with $ or ~"
        );
        ensure!(
            se == "F" || se.parse::<u8>().is_ok(),
            "expected non-literal map name to end with f or a number"
        );
        return assets.load_t2(&(sym[1..=3].to_owned() + ".T2"));
    }

    bail!("no map file matching {} found", raw)
}

// This is a slightly different problem then getting the T2, because even though ~ABCn.T2
// might exist for ~ABCn.MM, we need to look up FOOi.PIC.
pub fn get_base_name_for_map(raw: &str) -> Fallible<String> {
    let mut name = raw
        .split('.')
        .next()
        .ok_or_else(|| err_msg("expected a dotted name"))?;
    if name.starts_with('~') || name.starts_with('$') {
        name = &name[1..];
    }
    name = &name[0..3];
    let se = name.chars().rev().take(1).collect::<String>();
    if se.parse::<u8>().is_ok() {
        name = &name[..name.len() - 1];
    }

    Ok(name.to_owned())
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
    //let (mut pipeline, mut pds, mut vertex_buffer, mut index_buffer) = build_terrain_render_resources(&mm, assets.clone(), lib.clone(), &window, &vs, &fs)?;
    let renderer = TerrainRenderer::new(mm, &assets, &lib, &window)?;
    let (mut pipeline, mut pds, mut vertex_buffer, mut index_buffer) = renderer.render(&window)?;
    ///////////////////////////////////////////////////////////

    let mut push_constants = vs::ty::PushConstantData::new();

    let model = Isometry3::new(nalgebra::zero(), nalgebra::zero());
    let mut camera = ArcBallCamera::new(window.aspect_ratio()?);

    let mut need_reset = false;
    loop {
        if need_reset == true {
            need_reset = false;
            //let (pipeline2, pds2, vertex_buffer2, index_buffer2) = build_terrain_render_resources(&mm, assets.clone(), lib.clone(), &window, &vs, &fs)?;
            let (pipeline2, pds2, vertex_buffer2, index_buffer2) = renderer.render(&window)?;
            pipeline = pipeline2;
            pds = pds2;
            vertex_buffer = vertex_buffer2;
            index_buffer = index_buffer2;
        }

        push_constants.set_projection(camera.projection_for(model));

        window.drive_frame(|command_buffer, dynamic_state| {
            Ok(command_buffer.draw_indexed(
                pipeline.clone(),
                dynamic_state,
                vec![vertex_buffer.clone()],
                index_buffer.clone(),
                pds.clone(),
                push_constants,
            )?)
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

            // Keyboard
            DeviceEvent {
                event:
                    Key(KeyboardInput {
                        virtual_keycode: Some(keycode),
                        ..
                    }),
                ..
            } => match keycode {
                VirtualKeyCode::Escape => done = true,
                VirtualKeyCode::R => need_reset = true,
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
    }
}

struct TerrainRenderer {
    mm: MissionMap,
    terrain: Arc<Box<Terrain>>,
    layer: Arc<Box<Layer>>,
    pic_data: HashMap<TLoc, Vec<u8>>,
    base_palette: Palette,
    vs: vs::Shader,
    fs: fs::Shader,
}

impl TerrainRenderer {
    fn new(
        mm: MissionMap,
        assets: &Arc<Box<AssetManager>>,
        lib: &Arc<Box<Library>>,
        window: &GraphicsWindow,
    ) -> Fallible<Self> {
        let terrain = load_t2_for_map(&mm.map_name.to_uppercase(), assets, lib)?;

        // The following are used in FA:
        //    cloud1b.LAY 1
        //    day2b.LAY 0
        //    day2b.LAY 4
        //    day2e.LAY 0
        //    day2f.LAY 0
        //    day2.LAY 0
        //    day2v.LAY 0
        let layer = assets.load_lay(&mm.layer_name.to_uppercase())?;

        let mut pic_data = HashMap::new();
        let texture_base_name = get_base_name_for_map(&mm.map_name)?.to_uppercase();
        for (_pos, tmap) in &mm.tmaps {
            if !pic_data.contains_key(&tmap.loc) {
                let name = tmap.loc.pic_file(&texture_base_name);
                let data = lib.load(&name)?.to_vec();
                pic_data.insert(tmap.loc.clone(), data);
            }
        }

        let base_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

        let vs = vs::Shader::load(window.device())?;
        let fs = fs::Shader::load(window.device())?;

        Ok(Self {
            mm,
            terrain,
            layer,
            pic_data,
            base_palette,
            vs,
            fs,
        })
    }

    fn render(
        &self,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
        Arc<dyn DescriptorSet + Send + Sync>,
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
    )> {
        // Note: we need to really find the right palette.
        let mut palette = self.base_palette.clone();
        let layer_data = self.layer.for_index(self.mm.layer_index + 2);
        let r0 = layer_data.slice(0x00, 0x10)?;
        let r1 = layer_data.slice(0x10, 0x20)?;
        let r2 = layer_data.slice(0x20, 0x30)?;
        let r3 = layer_data.slice(0x30, 0x40)?;

        // We need to put rows r0, r1, and r2 into into 0xC0, 0xE0, 0xF0 somehow.
        palette.overlay_at(&r1, 0xF0)?;
        palette.overlay_at(&r0, 0xE0)?;

        // I'm pretty sure this is correct.
        palette.overlay_at(&r3, 0xD0)?;

        palette.overlay_at(&r2, 0xC0)?;
        //palette.overlay_at(&r2, 0xC1)?;

        palette.dump_png("terrain_palette")?;

        // Texture counts for all FA T2's.
        // APA: 68 x 256 (6815744 texels)
        // BAL: 66 x 256
        // CUB: 66 x 256
        // EGY: 49 x 256
        // FRA: 47 x 256
        // GRE: 68
        // IRA: 51 x 256
        // KURILE: 236 (Kxxxxxx) x 128/256 (33554432 texels)
        // LFA: 68
        // NSK: 68
        // PGU: 51
        // SPA: 49
        // TVIET: 42 (TVI) x 256
        // UKR: 29
        // VLA: 52
        // WTA: 68

        // Load all images with our new palette.
        let mut pics = Vec::new();
        for (tloc, data) in &self.pic_data {
            let pic = decode_pic(&palette, data)?;
            pics.push((tloc.clone(), pic));
        }

        let atlas = TextureAtlas::new(pics)?;

        let (texture, tex_future) = upload_texture_rgba(window, atlas.img.to_rgba())?;
        tex_future.then_signal_fence_and_flush()?.cleanup_finished();
        let sampler = make_sampler(window.device())?;

        let (vertex_buffer, index_buffer) =
            self.upload_terrain_textured_simple(&atlas, &palette, window)?;

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(self.vs.main_entry_point(), ())
                .triangle_strip()
                .cull_mode_back()
                .front_face_counter_clockwise()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(self.fs.main_entry_point(), ())
                .depth_stencil_simple_depth()
                .blend_alpha_blending()
                .render_pass(
                    Subpass::from(window.render_pass(), 0)
                        .expect("gfx: did not find a render pass"),
                )
                .build(window.device())?,
        );

        let pds = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())?
                .build()?,
        );

        Ok((pipeline, pds, vertex_buffer, index_buffer))
    }

    fn sample_at(&self, palette: &Palette, xi: u32, zi: u32) -> ([f32; 3], [f32; 4]) {
        let offset = (zi * self.terrain.width + xi) as usize;
        let s = if offset < self.terrain.samples.len() {
            self.terrain.samples[offset]
        } else {
            let offset = ((zi - 1) * self.terrain.width + xi) as usize;
            if offset < self.terrain.samples.len() {
                self.terrain.samples[offset]
            } else {
                let offset = ((zi - 1) * self.terrain.width + (xi - 1)) as usize;
                self.terrain.samples[offset]
            }
        };

        let x = xi as f32 / (self.terrain.width as f32) - 0.5;
        let z = zi as f32 / (self.terrain.height as f32) - 0.5;
        let h = -(s.height as f32) / (256.0f32 * 2f32);

        let mut c = palette.rgba(s.color as usize).unwrap();
        if s.color == 0xFF {
            c.data[3] = 0;
        }

        (
            [x, h, z],
            [
                c[0] as f32 / 255f32,
                c[1] as f32 / 255f32,
                c[2] as f32 / 255f32,
                c[3] as f32 / 255f32,
            ],
        )
    }

    fn upload_terrain_textured_simple(
        &self,
        atlas: &TextureAtlas,
        palette: &Palette,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
    )> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        for zi_base in (0..self.terrain.height).step_by(4) {
            for xi_base in (0..self.terrain.width).step_by(4) {
                let base = verts.len() as u32;

                // Upload all vertices in patch.
                if let Some(tmap) = self.mm.tmaps.get(&(xi_base, zi_base)) {
                    let frame = &atlas.frames[&tmap.loc];

                    for z_off in 0..5 {
                        for x_off in 0..5 {
                            let zi = zi_base + z_off;
                            let xi = xi_base + x_off;
                            let (position, samp_color) = self.sample_at(palette, xi, zi);
//                            let color = match tmap.orientation {
//                                MapOrientation::Unk0 => [0f32, 1f32, 0f32, 1f32],
//                                _ => samp_color,
//                            };

                            verts.push(Vertex {
                                position,
                                // FIXME: make this 0's once we understand mapping correctly.
                                color: [0f32, 0f32, 0f32, 0f32],
                                tex_coord: frame.interp(
                                    x_off as f32 / 4f32,
                                    z_off as f32 / 4f32,
                                    &tmap.orientation,
                                )?,
                            });
                        }
                    }
                } else {
                    for z_off in 0..5 {
                        for x_off in 0..5 {
                            let zi = zi_base + z_off;
                            let xi = xi_base + x_off;
                            let (position, color) = self.sample_at(palette, xi, zi);

                            verts.push(Vertex {
                                position,
                                color,
                                tex_coord: [0f32, 0f32],
                            });
                        }
                    }
                }

                // There is a fixed strip pattern here that we could probably make use of.
                // For now just re-compute per patch with the base offset.
                for row in 0..4 {
                    let row_off = row * 5;

                    indices.push(base + row_off + 0);
                    indices.push(base + row_off + 0);

                    for column in 0..5 {
                        indices.push(base + row_off + column);
                        indices.push(base + row_off + column + 5);
                    }

                    indices.push(base + row_off + 4 + 5);
                    indices.push(base + row_off + 4 + 5);
                }
            }
        }

        trace!(
            "uploading vertex buffer with {} bytes",
            std::mem::size_of::<Vertex>() * verts.len()
        );
        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

        trace!(
            "uploading index buffer with {} bytes",
            std::mem::size_of::<u32>() * indices.len()
        );
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        Ok((vertex_buffer, index_buffer))
    }

    fn upload_terrain_simple(
        &self,
        _atlas: &TextureAtlas,
        palette: &Palette,
        window: &GraphicsWindow,
    ) -> Fallible<(
        Arc<CpuAccessibleBuffer<[Vertex]>>,
        Arc<CpuAccessibleBuffer<[u32]>>,
    )> {
        let mut verts = Vec::new();
        for (i, s) in self.terrain.samples.iter().enumerate() {
            let i = i as u32;
            let x = (i % self.terrain.width) as f32 / (self.terrain.width as f32) - 0.5;
            let z = (i / self.terrain.width) as f32 / (self.terrain.height as f32) - 0.5;
            let h = -(s.height as f32) / (256.0f32 * 2f32);

            let mut c = palette.rgba(s.color as usize)?;
            if s.color == 0xFF {
                c.data[3] = 0;
            }

            verts.push(Vertex {
                position: [x, h, z],
                color: [
                    c[0] as f32 / 255f32,
                    c[1] as f32 / 255f32,
                    c[2] as f32 / 255f32,
                    c[3] as f32 / 255f32,
                ],
                tex_coord: [x, z],
            });
        }
        let vertex_buffer =
            CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

        let n_tris = (self.terrain.width - 1) * (self.terrain.height - 1) * 2;
        let mut indices: Vec<u32> = Vec::with_capacity(n_tris as usize + 2);
        indices.push(0u32);
        indices.push(self.terrain.width);
        for z in 0u32..(self.terrain.height - 1) {
            let zp0 = z * self.terrain.width;
            let zp1 = zp0 + self.terrain.width;
            for x in 0u32..self.terrain.width {
                indices.push(zp0 + x);
                indices.push(zp1 + x);
            }

            // Create some degenerate tris so that we can move the cursor without spraying triangles everywhere.
            indices.push(zp1 + self.terrain.width - 1);
            indices.push(zp1);
        }
        let index_buffer = CpuAccessibleBuffer::from_iter(
            window.device(),
            BufferUsage::all(),
            indices.into_iter(),
        )?;

        Ok((vertex_buffer, index_buffer))
    }
}

pub fn upload_texture_rgba(
    window: &GraphicsWindow,
    image_buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> Fallible<(Arc<ImmutableImage<Format>>, Box<GpuFuture>)> {
    let image_dim = image_buf.dimensions();
    let image_data = image_buf.into_raw().clone();

    let dimensions = Dimensions::Dim2d {
        width: image_dim.0,
        height: image_dim.1,
    };
    let (texture, tex_future) = ImmutableImage::from_iter(
        image_data.iter().cloned(),
        dimensions,
        Format::R8G8B8A8Srgb,
        window.queue(),
    )?;
    return Ok((texture, Box::new(tex_future) as Box<GpuFuture>));
}

pub fn make_sampler(device: Arc<Device>) -> Fallible<Arc<Sampler>> {
    let sampler = Sampler::new(
        device.clone(),
        Filter::Nearest,
        Filter::Nearest,
        MipmapMode::Nearest,
        SamplerAddressMode::ClampToEdge,
        SamplerAddressMode::ClampToEdge,
        SamplerAddressMode::ClampToEdge,
        0.0,
        1.0,
        0.0,
        0.0,
    )?;

    Ok(sampler)
}
