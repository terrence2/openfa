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
extern crate image;
extern crate kiss3d;
extern crate nalgebra as na;
extern crate pal;
extern crate pic;
extern crate t2;

use clap::{App, Arg, SubCommand};
use glfw::{Action, Key, WindowEvent};
use image::GenericImage;
use kiss3d::light::Light;
use kiss3d::resource::Mesh;
use kiss3d::scene::SceneNode;
use kiss3d::window::Window;
use na::{Point2, Point3, Translation3, UnitQuaternion, Vector3};
use pal::Palette;
use std::collections::HashMap;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::{cell, cmp, fs, mem, rc};
use t2::Terrain;

fn main() {
    let matches = App::new("OpenFA mission map explorer")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Figure out what bits belong where.")
        .arg(
            Arg::with_name("INPUT")
                .help("The mission map to show")
                .multiple(true)
                .required(true),
        )
        .get_matches();

    let files = get_files(matches.value_of("INPUT").unwrap());
    run_loop(files);
}

struct TextureInfo {
    name: String,
    source: String,
    cache: PathBuf,
    size: [f32; 2],
}

struct ViewState {
    files: Vec<PathBuf>,
    offset: usize,
    terrain: Terrain,
    mesh_nodes: Vec<SceneNode>,
    texture: TextureInfo,
    palette: Palette,
}

impl ViewState {
    fn new(files: Vec<PathBuf>, window: &mut Window) -> ViewState {
        let mut fp = fs::File::open("test_data/PALETTE.PAL").unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        let palette = Palette::from_bytes(&data).unwrap();

        let terrain = Self::_load_terrain(&files[0]);
        let texture = Self::preload_all_textures(&terrain, &palette);

        let mut state = ViewState {
            files,
            offset: 0,
            terrain,
            mesh_nodes: Vec::new(),
            texture,
            palette,
        };
        state._redraw(window);
        return state;
    }

    fn preload_all_textures(terrain: &Terrain, palette: &Palette) -> TextureInfo {
        let filename = terrain.pic_file.clone();
        println!("loading texture: {}", filename);

        let cache_name = Path::new(&format!("/tmp/{}.png", filename)).to_owned();
        let source = format!("test_data/{}", filename.to_uppercase());
        let mut fp = fs::File::open(source.clone()).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        let imagebuf = pic::decode_pic(palette, &data).unwrap();
        let ref mut fout = fs::File::create(&cache_name).unwrap();
        imagebuf.save(fout, image::PNG).unwrap();
        let tex_size = [
            imagebuf.dimensions().0 as f32,
            imagebuf.dimensions().1 as f32,
        ];
        return TextureInfo {
            name: filename.clone(),
            source: source,
            cache: cache_name,
            size: tex_size,
        };
    }

    fn _load_terrain(path: &PathBuf) -> Terrain {
        let mut fp = fs::File::open(path).unwrap();
        let mut data = Vec::new();
        fp.read_to_end(&mut data).unwrap();
        let terrain = Terrain::from_bytes(&data).unwrap();
        return terrain;
    }

    fn _remove_terrain(&mut self, window: &mut Window) {
        for mut node in self.mesh_nodes.iter_mut() {
            window.remove(&mut node);
        }
    }

    fn _redraw(&mut self, window: &mut Window) {
        self._remove_terrain(window);
        self.mesh_nodes = self._draw_terrain(window);
    }

    fn _draw_terrain(&mut self, window: &mut Window) -> Vec<SceneNode> {
        let mut nodes = Vec::new();

        println!("width : {}", self.terrain.width);
        println!("height: {}", self.terrain.height);

        let mut vert_buf = Vec::new();
        let mut index_buf = Vec::new();
        let mut uv_buf = Vec::new();

        let delta = 5f32;
        let x_base = -(self.terrain.width as f32) / 2f32 * delta;
        let z_base = -(self.terrain.height as f32) / 2f32 * delta;
        for (pos, sample) in self.terrain.samples.iter().enumerate() {
            let x_off = (pos % self.terrain.width) as f32;
            let z_off = (self.terrain.height - (pos / self.terrain.width) - 1) as f32;

            vert_buf.push(Point3::new(
                x_base + x_off * delta,
                sample.height.into(),
                z_base + z_off * delta,
            ));

            if x_off as u32 % 10 == 0 && z_off as u32 % 10 == 0 {
                println!(
                    "Adding sphere at {}, {}, {}",
                    x_base + x_off * delta,
                    sample.height,
                    z_base + z_off * delta,
                );
                let mut node = window.add_sphere(0.5);
                node.append_translation(&Translation3::new(
                    x_base + x_off * delta,
                    sample.height.into(),
                    z_base + z_off * delta,
                ));
                nodes.push(node);
            }

            uv_buf.push(Point2::new(
                x_off / self.terrain.width as f32,
                z_off / self.terrain.height as f32,
            ));
        }

        for (pos, sample) in self.terrain.samples.iter().enumerate() {
            let x_off = (pos % self.terrain.width) as f32;
            let z_off = (self.terrain.height - (pos / self.terrain.width) - 1) as f32;

            let max_ask = pos + self.terrain.width + 1;
            if max_ask < vert_buf.len() {
                index_buf.push(Point3::new(
                    pos as u32 + 1,
                    pos as u32 + self.terrain.width as u32,
                    pos as u32,
                ));
                index_buf.push(Point3::new(
                    pos as u32 + self.terrain.width as u32 + 1u32,
                    pos as u32 + self.terrain.width as u32,
                    pos as u32 + 1u32,
                ));
            }
        }

        let m = rc::Rc::new(cell::RefCell::new(Mesh::new(
            vert_buf,
            index_buf,
            None,
            Some(uv_buf),
            false,
        )));
        let mut node = window.add_mesh(m, Vector3::new(1.0, 1.0, 1.0));
        node.set_texture_from_file(&self.texture.cache, &self.texture.name);
        nodes.push(node);

        println!("fin");
        return nodes;
    }

    fn next_object(&mut self, window: &mut Window) {
        self.offset += 1;
        self.offset %= self.files.len();
        self._use_terrain(window);
    }

    fn prev_object(&mut self, window: &mut Window) {
        if self.offset > 0 {
            self.offset -= 1;
        } else {
            self.offset = self.files.len() - 1;
        }
        self._use_terrain(window);
    }

    fn _use_terrain(&mut self, window: &mut Window) {
        self.terrain = Self::_load_terrain(&self.files[self.offset]);
        self._redraw(window)
    }
}

fn run_loop(files: Vec<PathBuf>) {
    let mut window = Window::new("Kiss3d: terrain");
    let mut state = ViewState::new(files, &mut window);

    window.set_light(Light::StickToCamera);

    while window.render() {
        for mut event in window.events().iter() {
            event.inhibited = false;
            match event.value {
                WindowEvent::Key(Key::PageDown, _, Action::Press, _) => {
                    state.next_object(&mut window);
                }
                WindowEvent::Key(Key::PageUp, _, Action::Press, _) => {
                    state.prev_object(&mut window);
                }
                // WindowEvent::Key(Key::Up, _, Action::Press, _) => {
                //     state.next_instr_10(&mut window);
                // }
                // WindowEvent::Key(Key::Down, _, Action::Press, _) => {
                //     state.prev_instr_10(&mut window);
                // }
                // WindowEvent::Key(Key::Right, _, Action::Press, _) => {
                //     state.next_instr(&mut window);
                // }
                // WindowEvent::Key(Key::Left, _, Action::Press, _) => {
                //     state.prev_instr(&mut window);
                // }
                // WindowEvent::Key(Key::Right, _, Action::Repeat, _) => {
                //     state.next_instr(&mut window);
                // }
                // WindowEvent::Key(Key::Left, _, Action::Repeat, _) => {
                //     state.prev_instr(&mut window);
                // }
                // WindowEvent::Key(Key::End, _, Action::Press, _) => {
                //     state.last_instr(&mut window);
                // }
                // WindowEvent::Key(Key::Home, _, Action::Press, _) => {
                //     state.first_instr(&mut window);
                // }
                _ => {}
            }
        }
    }
}

fn get_files(input: &str) -> Vec<PathBuf> {
    let path = Path::new(input);
    if path.is_dir() {
        return path.read_dir()
            .unwrap()
            .map(|p| p.unwrap().path().to_owned())
            .collect::<Vec<_>>();
    }
    return vec![path.to_owned()];
}
