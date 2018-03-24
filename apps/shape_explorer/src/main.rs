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
extern crate clap;
extern crate glfw;
extern crate kiss3d;
extern crate nalgebra as na;
extern crate pal;
extern crate pic;
extern crate sh;
extern crate image;

use image::GenericImage;
use clap::{Arg, App, SubCommand};
use glfw::{Action, Key, WindowEvent};
use sh::{CpuShape, FacetFlags, Instr};
use std::{cmp, cell, fs, rc};
use std::path::{Path, PathBuf};
use std::io::prelude::*;
use pal::Palette;
use na::{Point2, Point3, Vector3, UnitQuaternion, Translation3};
use kiss3d::window::Window;
use kiss3d::light::Light;
use kiss3d::scene::SceneNode;
use kiss3d::resource::Mesh;

fn main() {
    let matches = App::new("OpenFA shape explorer")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Figure out what bits belong where.")
        .arg(Arg::with_name("INPUT")
            .help("The shape(s) to show")
            .required(true))
        .get_matches();

    let files = get_files(matches.value_of("INPUT").unwrap());
    run_loop(files);
}

struct ViewState {
    files: Vec<PathBuf>,
    offset: usize,
    shape: CpuShape,
    mesh_nodes: Vec<SceneNode>,
    //textures: HashMap<String, String>,
    palette: Palette,
    tex_size: [f32;2],// [0f32, 0f32];
    active_mesh: usize,
    instr_count: usize,
}

impl ViewState {
    fn new(files: Vec<PathBuf>, window: &mut Window) -> ViewState {
        let mut fp = fs::File::open("test_data/PALETTE.PAL").unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        let palette = Palette::from_bytes(&data).unwrap();

        let shape = Self::_load_shape(&files[0]);

        let mut state = ViewState {
            files,
            offset: 0,
            shape,
            mesh_nodes: Vec::new(),
            palette,
            tex_size: [0f32, 0f32],
            active_mesh: 0,
            instr_count: 284,
        };
        state._redraw(window);
        state.set_vertex_colors();
        return state;
    }

    fn _load_shape(path: &PathBuf) -> CpuShape {
        let mut fp = fs::File::open(path).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        let shape = CpuShape::new(&data).unwrap();
        return shape;
    }

    fn _redraw(&mut self, window: &mut Window) {
        self._remove_shape(window);
        self.mesh_nodes = self._draw_shape(window);
    }

    fn _draw_shape(&mut self, window: &mut Window) -> Vec<SceneNode> {
        let mut nodes = Vec::new();

        let mut active_texture: Option<(String, PathBuf)> = None;

        let mut vert_buf: Vec<Point3<f32>> = Vec::new();

        for (i, instr) in self.shape.instrs.iter().enumerate() {
//            if i < 990 {
//                continue;
//            }
            if i == self.instr_count {
                break;
            }
            //if i == self.instr_count - 1 {
            println!("At: {} => {}", i, instr.show());
            //}
            match instr {
                &Instr::TextureRef(ref texture) => {
                    let cache_name = Path::new(&format!("/tmp/{}.png", texture.filename)).to_owned();
                    if active_texture.is_none() || !cache_name.exists() {
                        let source = format!("test_data/{}", texture.filename.to_uppercase());
                        let mut fp = fs::File::open(source).unwrap();
                        let mut data = Vec::new();
                        fp.read_to_end(&mut data).unwrap();
                        let imagebuf = pic::decode_pic(&self.palette, &data).unwrap();
                        let ref mut fout = fs::File::create(&cache_name).unwrap();
                        imagebuf.save(fout, image::PNG).unwrap();
                        self.tex_size = [imagebuf.dimensions().0 as f32, imagebuf.dimensions().1 as f32];
                    }
                    active_texture = Some((texture.filename.clone(), cache_name));
                }
                &Instr::VertexBuf(ref buf) => {
//                    if i == 990 {
//                        vert_buf.truncate(0);
//                    }
                    for v in buf.verts.iter() {
                        vert_buf.push(Point3::new(v[0], v[1], v[2]));
                    }
                    for v in buf.verts.iter() {
                        let mut node = window.add_sphere(0.5);
                        node.append_translation(&Translation3::new(v[0], v[1], v[2]));
                        nodes.push(node);
                    }
                }
                &Instr::Facet(ref facet) => {
                    let mut coords = Vec::new();
                    let mut index_buf = Vec::new();
                    let mut uv_buf: Option<Vec<Point2<f32>>> = None;
                    if facet.tex_coords.len() > 0 {
                        uv_buf = Some(Vec::new());
                    }
                    for base in 2..facet.indices.len() {
                        let coords_base = coords.len() as u32;
                        index_buf.push(Point3::new(coords_base,
                                                   coords_base + 1,
                                                   coords_base + 2));
                        coords.push(vert_buf[facet.indices[0] as usize]);
                        coords.push(vert_buf[facet.indices[base - 0] as usize]);
                        coords.push(vert_buf[facet.indices[base - 1] as usize]);
                        if let Some(ref mut uvs) = uv_buf {
                            uvs.push(Point2::new(facet.tex_coords[0][0] as f32 / self.tex_size[0],
                                                 1f32 - facet.tex_coords[0][1] as f32 / self.tex_size[1]));
                            uvs.push(Point2::new(facet.tex_coords[base - 0][0] as f32 / self.tex_size[0],
                                                 1f32 - facet.tex_coords[base - 0][1] as f32 / self.tex_size[1]));
                            uvs.push(Point2::new(facet.tex_coords[base - 1][0] as f32 / self.tex_size[0],
                                                 1f32 - facet.tex_coords[base - 1][1] as f32 / self.tex_size[1]));
                        }
                    }

                    let m = rc::Rc::new(cell::RefCell::new(Mesh::new(coords, index_buf, None, uv_buf, false)));
                    let mut node = window.add_mesh(m, Vector3::new(1.0, 1.0, 1.0));
                    match &active_texture {
                        &None => (),
                        &Some((ref name, ref path)) => node.set_texture_from_file(path, name)
                    }
                    nodes.push(node);
                }
                _ => {}
            }
        }

        return nodes;
    }

//    fn _push_shape_vertices(window: &mut Window, shape: &Shape) -> Vec<SceneNode> {
//        let mut vertex_nodes = Vec::new();
//        for v in shape.vertices.iter() {
//            let mut node = window.add_sphere(0.5);
//            node.append_translation(&Translation3::new(v[0], v[1], v[2]));
//            vertex_nodes.push(node);
//        }
//        return vertex_nodes;
//    }

//    fn _push_shape_meshes(window: &mut Window, shape: &Shape) -> Vec<SceneNode> {
//        let mut nodes = Vec::new();
//
//        for (i, mesh) in shape.meshes.iter().enumerate() {
//            for v in mesh.vertices.iter() {
//                let mut node = window.add_sphere(0.5);
//                node.append_translation(&Translation3::new(v[0], v[1], v[2]));
//                nodes.push(node);
//            }
//
//            for facet in mesh.facets.iter() {
//                for index in facet.indices.iter() {
//                    println!("{}: {} of {}", i, index, mesh.vertices.len());
//                }
//            }
//
//            let mut vert_buf = Vec::new();
//            for v in mesh.vertices.iter() {
//                vert_buf.push(Point3::new(v[0], v[1], v[2]));
//            }
//
//            let mut index_buf = Vec::new();
//            for facet in mesh.facets.iter() {
//                assert!(facet.indices.len() >= 3);
//                for base in 2..facet.indices.len() {
//                    let i = facet.indices[0] as u32;
//                    let j = facet.indices[base - 1] as u32;
//                    let k = facet.indices[base - 0] as u32;
//                    index_buf.push(Point3::new(k, j, i));
//                }
//            }
//
//            if index_buf.len() > 0 {
//                let m = rc::Rc::new(cell::RefCell::new(Mesh::new(vert_buf, index_buf, None, None, false)));
//                let node = window.add_mesh(m, Vector3::new(1.0, 1.0, 1.0));
//                nodes.push(node);
//            }
//        }
//
//        return nodes;
//    }

    fn _remove_shape(&mut self, window: &mut Window) {
//        for mut node in self.vertex_nodes.iter_mut() {
//            window.remove(&mut node);
//        }
        for mut node in self.mesh_nodes.iter_mut() {
            window.remove(&mut node);
        }
    }

    fn next_shape(&mut self, window: &mut Window) {
        self.offset += 1;
        self.offset %= self.files.len();
        self._use_shape(window);
    }

    fn prev_shape(&mut self, window: &mut Window) {
        if self.offset > 0 {
            self.offset -= 1;
        } else {
            self.offset = self.files.len() - 1;
        }
        while self.offset < 0 { self.offset += self.files.len(); }
        self._use_shape(window);
    }

    fn _use_shape(&mut self, window: &mut Window) {
        self.active_mesh = 0;
        self.instr_count = 0;
        self.shape = Self::_load_shape(&self.files[self.offset]);
        self._redraw(window)
    }

    fn next_instr(&mut self, window: &mut Window) {
        self.instr_count += 1;
        self.instr_count = cmp::min(self.instr_count, self.shape.instrs.len());
        self._redraw(window);
    }

    fn prev_instr(&mut self, window: &mut Window) {
        self.instr_count -= 1;
        self.instr_count = cmp::max(self.instr_count, 0);
        self._redraw(window);
    }

    fn next_instr_10(&mut self, window: &mut Window) {
        self.instr_count += 10;
        self.instr_count = cmp::min(self.instr_count, self.shape.instrs.len());
        self._redraw(window);
    }

    fn prev_instr_10(&mut self, window: &mut Window) {
        if self.instr_count >= 0 {
            self.instr_count -= 10;
        } else {
            self.instr_count = 0;
        }
        self.instr_count = cmp::max(self.instr_count, 0);
        self._redraw(window);
    }

    fn last_instr(&mut self, window: &mut Window) {
        self.instr_count = self.shape.instrs.len();
        self._redraw(window);
    }

    fn first_instr(&mut self, window: &mut Window) {
        self.instr_count = 0;
        self._redraw(window);
    }

    fn set_vertex_colors(&mut self) {
//        let active_facet = &self.shape.meshes[self.active_mesh].facets[self.active_face];
//        for (i, mut node) in self.vertex_nodes.iter_mut().enumerate() {
//            let mut c = [0.1, 0.1, 0.1];
//            let mut s = 0.5;
//            for (j, &index) in active_facet.indices.iter().enumerate() {
//                if i == (index as usize) {
//                    s = 1.0;
//                    c = match j {
//                        0 => [1.0, 0.0, 0.0],
//                        1 => [0.0, 1.0, 0.0],
//                        2 => [0.0, 0.0, 1.0],
//                        3 => [1.0, 0.5, 0.0],
//                        _ => [1.0, 0.0, 1.0],
//                    }
//                }
//            }
//            node.set_local_scale(s, s, s);
//            node.set_color(c[0], c[1], c[2]);
//        }
    }
}

fn run_loop(files: Vec<PathBuf>) {
    let mut window = Window::new("Kiss3d: shape");
    let mut state = ViewState::new(files, &mut window);

    window.set_light(Light::StickToCamera);

    while window.render() {
        for mut event in window.events().iter() {
            event.inhibited = false;
            match event.value {
                WindowEvent::Key(Key::PageDown, _, Action::Press, _) => {
                    state.next_shape(&mut window);
                },
                WindowEvent::Key(Key::PageUp, _, Action::Press, _) => {
                    state.prev_shape(&mut window);
                },
                WindowEvent::Key(Key::Up, _, Action::Press, _) => {
                    state.next_instr_10(&mut window);
                },
                WindowEvent::Key(Key::Down, _, Action::Press, _) => {
                    state.prev_instr_10(&mut window);
                },
                WindowEvent::Key(Key::Right, _, Action::Press, _) => {
                    state.next_instr(&mut window);
                },
                WindowEvent::Key(Key::Left, _, Action::Press, _) => {
                    state.prev_instr(&mut window);
                },
                WindowEvent::Key(Key::Right, _, Action::Repeat, _) => {
                    state.next_instr(&mut window);
                },
                WindowEvent::Key(Key::Left, _, Action::Repeat, _) => {
                    state.prev_instr(&mut window);
                },
                WindowEvent::Key(Key::End, _, Action::Press, _) => {
                    state.last_instr(&mut window);
                },
                WindowEvent::Key(Key::Home, _, Action::Press, _) => {
                    state.first_instr(&mut window);
                },
                _ => {},
            }
        }
    }
}

fn get_files(input: &str) -> Vec<PathBuf> {
    let path = Path::new(input);
    if path.is_dir() {
        return path.read_dir().unwrap().map(|p| { p.unwrap().path().to_owned() }).collect::<Vec<_>>();
    }
    return vec![path.to_owned()];
}

