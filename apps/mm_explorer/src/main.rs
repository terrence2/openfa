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
use failure::Fallible;
use omnilib::OmniLib;
use mm::MissionMap;
use nalgebra::{Isometry3, Matrix4, Orthographic3, Perspective3, Point3, Rotation3, Vector3};
use pal::Palette;
use std::{
    f32::consts::PI,
    path::{Path, PathBuf},
    sync::Arc,
};
use structopt::StructOpt;
use t2::Terrain;
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    descriptor::descriptor_set::PersistentDescriptorSet,
    framebuffer::Subpass,
    impl_vertex,
    pipeline::GraphicsPipeline,
    sync::GpuFuture,
};
use window::{GraphicsConfigBuilder, GraphicsWindow};
use xt::TypeManager;

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl_vertex!(Vertex, position, color);

mod vs {
    use vulkano_shaders::shader;

    shader! {
        ty: "vertex",
        src: "
            #version 450

            layout(location = 0) in vec3 position;
            layout(location = 1) in vec3 color;

            layout(push_constant) uniform PushConstantData {
              mat4 projection;
            } pc;

            layout(location = 0) out vec4 v_color;

            void main() {
                gl_Position = pc.projection * vec4(position, 1.0);
                v_color = vec4(color, 1.0);
                //tex_coords = position + vec2(1.0);
            }"
    }
}

mod fs {
    use vulkano_shaders::shader;

    shader! {
        ty: "fragment",
        src: "
            #version 450

            //layout(location = 0) in vec2 tex_coords;
            layout(location = 0) in vec4 v_color;

            layout(location = 0) out vec4 f_color;

            //layout(set = 0, binding = 0) uniform sampler2D tex;

            void main() {
                //f_color = texture(tex, tex_coords);
                f_color = v_color;
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

pub struct ArcBallCamera {
    target: Point3<f32>,
    distance: f32,
    yaw: f32,
    pitch: f32,
    projection: Perspective3<f32>,
}

impl ArcBallCamera {
    pub fn new(aspect_ratio: f32) -> Self {
        Self {
            target: Point3::new(0f32, 0f32, 0f32),
            distance: 1f32,
            yaw: 0f32,
            pitch: PI / 2f32,
            projection: Perspective3::new(1f32 / aspect_ratio, PI / 2f32, 0.01f32, 10.0f32),
        }
    }

    fn eye(&self) -> Point3<f32> {
        let px = self.target.x + self.distance * self.yaw.cos() * self.pitch.sin();
        let py = self.target.y + self.distance * self.pitch.cos();
        let pz = self.target.z + self.distance * self.yaw.sin() * self.pitch.sin();
        Point3::new(px, py, pz)
    }

    fn view(&self) -> Isometry3<f32> {
        Isometry3::look_at_rh(&self.eye(), &self.target, &Vector3::y())
    }

    pub fn projection_for(&self, model: Isometry3<f32>) -> Matrix4<f32> {
        self.projection.as_matrix() * (model * self.view()).to_homogeneous()
    }

    pub fn on_mousemove(&mut self, x: f32, y: f32) {
        self.yaw += x as f32 * 0.5 * (3.14 / 180.0);

        self.pitch += y as f32 * (3.14 / 180.0);
        self.pitch = self.pitch.min(PI - 0.001f32).max(0.001f32);
    }
}


#[derive(Debug, StructOpt)]
#[structopt(name = "mm_explorer", about = "Show the contents of an mm file")]
struct Opt {
    #[structopt(short="g", long="game", default_value="FA", help="The game libraries to load.")]
    game: String,

    #[structopt(help="Will load it from game, or look at last component of path")]
    input: String,
}

pub fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    let omnilib = OmniLib::new_for_test()?;
    let lib = omnilib.library(&opt.game);

    let mut window = GraphicsWindow::new(&GraphicsConfigBuilder::new().build())?;

    let mut base_palette = Palette::from_bytes(&lib.load("PALETTE.PAL")?)?;

    let assets = Arc::new(Box::new(AssetManager::new(lib.clone())?));
    let types = TypeManager::new(lib.clone(), assets.clone())?;
    let contents = lib.load_text(&opt.input)?;
    let mm = MissionMap::from_str(&contents, lib.clone(), &types, assets.clone())?;

    let terrain = mm.map;

    // FIXME: this is 100% wrong.
    // I think we want to copy the last 3 rows 1 row up; Then we need to use something to find
    // to detect where water is and do... something.
    let layer_data = mm.layer.for_index(mm.layer_index + 2);
    base_palette.overlay_at(layer_data, 0xC0)?;
    let slice = layer_data.slice(0x20, 0x30)?;
    base_palette.overlay_at(&slice, 0xD0)?;

    base_palette.dump_png("base_palette")?;

    let mut verts = Vec::new();
    for (i, s) in terrain.samples.iter().enumerate() {
        let i = i as u32;
        let x = (i % terrain.width) as f32 / (terrain.width as f32) - 0.5;
        let z = (i / terrain.width) as f32 / (terrain.height as f32) - 0.5;
        let h = -(s.height as f32) / (256.0f32 * 2f32);

        //        let mut metaclr = if s.modifiers == 16 {
        //            [1f32, 0f32, 1f32]
        //        } else {
        //            [
        //                (s.modifiers as f32 * 18f32) / 256f32,
        //                (s.modifiers as f32 * 18f32) / 256f32,
        //                (s.modifiers as f32 * 18f32) / 256f32,
        //            ]
        //        };

        let c = base_palette.rgb(s.color as usize)?;
        //println!("Mapped 0x{:02X} to {:?}", s.color, c);

        verts.push(Vertex {
            position: [x, h, z],
            color: [c[0] as f32 / 255f32, c[1] as f32 / 255f32, c[2] as f32 / 255f32],
        });
    }
    let vertex_buffer =
        CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), verts.into_iter())?;

    let n_tris = (terrain.width - 1) * (terrain.height - 1) * 2;
    let mut indices: Vec<u32> = Vec::with_capacity(n_tris as usize + 2);
    indices.push(0u32);
    indices.push(terrain.width);
    for z in 0u32..(terrain.height - 1) {
        let zp0 = z * terrain.width;
        let zp1 = zp0 + terrain.width;
        for x in 0u32..terrain.width {
            indices.push(zp0 + x);
            indices.push(zp1 + x);
        }

        // Create some degenerate tris so that we can move the cursor without spraying triangles everywhere.
        indices.push(zp1 + terrain.width - 1);
        indices.push(zp1);
    }
    let index_buffer =
        CpuAccessibleBuffer::from_iter(window.device(), BufferUsage::all(), indices.into_iter())?;

    let vs = vs::Shader::load(window.device())?;
    let fs = fs::Shader::load(window.device())?;

    let mut push_constants = vs::ty::PushConstantData::new();

    //let projection = Orthographic3::new(-1.0f32, 1.0f32, -1.0f32, 1.0f32, -1.0f32, 1.0f32);
    //push_constants.set_projection(projection.to_homogeneous());

    let model = Isometry3::new(nalgebra::zero(), nalgebra::zero());

    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input_single_buffer::<Vertex>()
            .vertex_shader(vs.main_entry_point(), ())
            .triangle_strip()
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(fs.main_entry_point(), ())
            .depth_stencil_simple_depth()
            .blend_alpha_blending()
            .render_pass(
                Subpass::from(window.render_pass(), 0).expect("gfx: did not find a render pass"),
            ).build(window.device())?,
    );

    //    let set = Arc::new(
    //        PersistentDescriptorSet::start(pipeline.clone(), 0)
    //            .add_sampled_image(texture.clone(), sampler.clone())?
    //            .build()?,
    //    );

    let mut camera = ArcBallCamera::new(window.aspect_ratio()?);

    // Our camera looks toward the point (1.0, 0.0, 0.0).
    // It is located at (0.0, 0.0, 1.0).
    //let mut eye = Point3::new(0.0, 0.0, -1.0);
    loop {
        push_constants.set_projection(camera.projection_for(model));

        window.drive_frame(|command_buffer, dynamic_state| {
            Ok(command_buffer.draw_indexed(
                pipeline.clone(),
                dynamic_state,
                vertex_buffer.clone(),
                index_buffer.clone(),
                (),
                push_constants,
            )?)
        })?;

        use winit::{
            DeviceEvent::{Key, MouseMotion},
            Event::{DeviceEvent, WindowEvent},
            KeyboardInput, VirtualKeyCode,
            WindowEvent::{CloseRequested, Destroyed, Resized},
        };

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

            // Keyboard
            DeviceEvent {
                event:
                Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::Escape),
                        ..
                    }),
                ..
            } => done = true,

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
