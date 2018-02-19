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
extern crate shape;

use clap::{Arg, App, SubCommand};
use glfw::{Action, Key, WindowEvent};
use shape::{Shape, ShowMode};
use std::{fs};
use std::path::{Path, PathBuf};
use std::io::prelude::*;
use na::{Vector3, UnitQuaternion, Translation3};
use kiss3d::window::Window;
use kiss3d::light::Light;
use kiss3d::scene::SceneNode;

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
    shape: Shape,
    vertex_nodes: Vec<SceneNode>,
    active_mesh: usize,
    active_face: usize,
}

impl ViewState {
    fn new(files: Vec<PathBuf>, window: &mut Window) -> ViewState {
        let shape = Self::_load_shape(&files[0]);
        let vertex_nodes = Self::_push_shape_vertices(window, &shape);
        let mut state = ViewState {
            files,
            offset: 0,
            shape,
            vertex_nodes,
            active_mesh: 0,
            active_face: 0,
        };
        state.show_active_mesh();
        return state;
    }

    fn _load_shape(path: &PathBuf) -> Shape {
        let mut fp = fs::File::open(path).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        let (shape, _desc) = Shape::new(path.to_str().unwrap(), &data, ShowMode::Unknown).unwrap();
        return shape;
    }

    fn _push_shape_vertices(window: &mut Window, shape: &Shape) -> Vec<SceneNode> {
        let mut vertex_nodes = Vec::new();
        for v in shape.vertices.iter() {
            let mut node = window.add_sphere(0.5);
            node.append_translation(&Translation3::new(v[0], v[1], v[2]));
            vertex_nodes.push(node);
        }
        return vertex_nodes;
    }

    fn _remove_shape(&mut self, window: &mut Window) {
        for mut node in self.vertex_nodes.iter_mut() {
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
        self._remove_shape(window);
        self.active_mesh = 0;
        self.active_face = 0;
        self.shape = Self::_load_shape(&self.files[self.offset]);
        self.vertex_nodes = Self::_push_shape_vertices(window, &self.shape);
        println!("showing {:?} w/ {} verts in {} meshes", self.files[self.offset], self.shape.vertices.len(), self.shape.meshes.len());
    }

    fn next_mesh(&mut self) {
        self.active_mesh += 1;
        self.active_mesh %= self.shape.meshes.len();
        self.active_face = 0;
        self.show_active_mesh();
        println!("Showing mesh {} with {} faces", self.active_mesh,
                 self.shape.meshes[self.active_mesh].facets.len());
    }

    fn prev_mesh(&mut self) {
        if self.active_mesh > 0 {
            self.active_mesh -= 1;
        } else {
            self.active_mesh = self.shape.meshes.len() - 1;
        }
        self.active_face = 0;
        self.show_active_mesh();
        println!("Showing mesh {} with {} faces", self.active_mesh,
                 self.shape.meshes[self.active_mesh].facets.len());
    }

    fn next_facet(&mut self) {
        self.active_face += 1;
        self.active_face %= self.shape.meshes[self.active_mesh].facets.len();
        self.show_active_mesh();
        println!("Highlighting facet {} with {} indices: {:?}", self.active_face,
                 self.shape.meshes[self.active_mesh].facets[self.active_face].indices.len(),
                 self.shape.meshes[self.active_mesh].facets[self.active_face].indices);
    }

    fn prev_facet(&mut self) {
        if self.active_face > 0 {
            self.active_face -= 1;
        } else {
            self.active_face = self.shape.meshes[self.active_mesh].facets.len() - 1;
        }
        self.show_active_mesh();
        println!("Highlighting facet {} with {} indices: {:?}", self.active_face,
                 self.shape.meshes[self.active_mesh].facets[self.active_face].indices.len(),
                 self.shape.meshes[self.active_mesh].facets[self.active_face].indices);
    }

    fn show_active_mesh(&mut self) {
        let active_facet = &self.shape.meshes[self.active_mesh].facets[self.active_face];
        for (i, mut node) in self.vertex_nodes.iter_mut().enumerate() {
            let c = if active_facet.indices.contains(&(i as u16)) {
                [1.0, 0.0, 1.0]
            } else {
                [0.1, 0.1, 0.1]
            };
            node.set_color(c[0], c[1], c[2]);
        }
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
                WindowEvent::Key(Key::Right, _, Action::Press, _) => {
                    state.next_mesh();
                },
                WindowEvent::Key(Key::Left, _, Action::Press, _) => {
                    state.prev_mesh();
                },
                WindowEvent::Key(Key::Up, _, Action::Press, _) => {
                    state.next_facet();
                },
                WindowEvent::Key(Key::Down, _, Action::Press, _) => {
                    state.prev_facet();
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

