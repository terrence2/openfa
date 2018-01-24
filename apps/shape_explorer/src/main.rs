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
extern crate kiss3d;
extern crate nalgebra as na;
extern crate shape;

use clap::{Arg, App, SubCommand};
use shape::Shape;
use std::{fs};
use std::io::prelude::*;
use na::{Vector3, UnitQuaternion, Translation3};
use kiss3d::window::Window;
use kiss3d::light::Light;

fn main() {
    let matches = App::new("OpenFA shape explorer")
        .version("0.0.1")
        .author("Terrence Cole <terrence.d.cole@gmail.com>")
        .about("Figure out what bits belong where.")
        .arg(Arg::with_name("INPUT")
            .help("The shape(s) to show")
            .required(true))
        .get_matches();

    let path = matches.value_of("INPUT").unwrap();
    let mut fp = fs::File::open(path).unwrap();
    let mut data = Vec::new();
    fp.read_to_end(&mut data).unwrap();
    let sh = Shape::new(path, &data).unwrap();

    let mut window = Window::new("Kiss3d: shape");
    for v in sh.vertices.iter() {
        let mut p = window.add_sphere(0.005);
        p.set_color(1.0, 0.0, 0.0);
        const SCALE: f32 = 1f32 / 32767f32 * 10f32;
        p.append_translation(&Translation3::new(v[0] as f32 * SCALE, v[1] as f32 * SCALE, v[2] as f32 * SCALE));
    }

    window.set_light(Light::StickToCamera);

    let rot = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), 0.014);

    while window.render() {
        //c.prepend_to_local_rotation(&rot);
    }
}
