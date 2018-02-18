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
use shape::Shape;
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
    run_kiss(files);
}

fn run_kiss(files: Vec<PathBuf>) {
    let mut offset = 0;
    let mut window = Window::new("Kiss3d: shape");
    let mut nodes = add_shape(&mut window, &load_file(&files[offset]));

    window.set_light(Light::StickToCamera);

    let rot = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), 0.014);

    while window.render() {
        for mut event in window.events().iter() {
            event.inhibited = false;
            match event.value {
                WindowEvent::Key(Key::PageDown, _, Action::Press, _) => {
                    offset += 1;
                    offset %= files.len();
                    remove_shape(&mut window, nodes);
                    let shape = &load_file(&files[offset]);
                    nodes = add_shape(&mut window, &shape);
                    println!("showing {:?} w/ {} verts", files[offset], shape.vertices.len());
                },
                WindowEvent::Key(Key::PageUp, _, Action::Press, _) => {
                    offset -= 1;
                    while offset < 0 { offset += files.len(); }
                    remove_shape(&mut window, nodes);
                    let shape = &load_file(&files[offset]);
                    nodes = add_shape(&mut window, &shape);
                    println!("showing {:?} w/ {} verts", files[offset], shape.vertices.len());
                },
                _ => {},
            }
        }
    }
}

fn load_file(path: &Path) -> Shape {
    let mut fp = fs::File::open(path).unwrap();
    let mut data = Vec::new();
    fp.read_to_end(&mut data).unwrap();
    let (shape, _desc) = Shape::new(path.to_str().unwrap(), &data).unwrap();
    return shape;
}

fn add_shape(window: &mut Window, shape: &Shape) -> Vec<SceneNode> {
    let mut nodes = Vec::new();
    for v in shape.vertices.iter() {
        let mut node = window.add_sphere(0.5);
        node.set_color(1.0, 1.0, 1.0);
        node.append_translation(&Translation3::new(v[0], v[1], v[2]));
        nodes.push(node);
    }
    return nodes;
}

fn remove_shape(window: &mut Window, nodes: Vec<SceneNode>) {
    for mut node in nodes {
        window.remove(&mut node);
    }
}

fn get_files(input: &str) -> Vec<PathBuf> {
    let path = Path::new(input);
    if path.is_dir() {
        return path.read_dir().unwrap().map(|p| { p.unwrap().path().to_owned() }).collect::<Vec<_>>();
    }
    return vec![path.to_owned()];
}

