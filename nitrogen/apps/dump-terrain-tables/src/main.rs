// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use failure::Fallible;
use nalgebra::Vector2;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fmt,
};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dump-terrain-tables",
    about = "Build tables from Kooima's thesis that are required for rendering planetary scale terrain."
)]
struct Opt {
    /// Dump all wireframe strips.
    #[structopt(short = "w", long)]
    dump_wireframe_list: bool,

    /// Dump all triangle strips.
    #[structopt(short = "s", long)]
    dump_triangle_strips: bool,

    /// Dump all index dependency LUTs
    #[structopt(short = "l", long)]
    dump_index_dependency_luts: bool,

    /// Select the max subdivision depth to dump.
    #[structopt(short, long, default_value = "8")]
    max_level: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct Edge {
    a: u32,
    b: u32,
}

impl Edge {
    fn new(i0: u32, i1: u32) -> Self {
        if i0 < i1 {
            Self { a: i0, b: i1 }
        } else {
            Self { a: i1, b: i0 }
        }
    }
}

impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.a, self.b,)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Triangle {
    v: [Vector2<i64>; 3],
    i: [u32; 3],
}

impl Triangle {
    fn new(
        i0: u32,
        v0: Vector2<i64>,
        i1: u32,
        v1: Vector2<i64>,
        i2: u32,
        v2: Vector2<i64>,
    ) -> Self {
        Self {
            v: [v0, v1, v2],
            i: [i0, i1, i2],
        }
    }

    fn leftmost(&self) -> i64 {
        let a = self.v[0][0];
        let b = self.v[1][0];
        let c = self.v[2][0];
        a.min(b).min(c)
    }

    fn rightmost_index(&self) -> u32 {
        // Note: we are never aligned on x, so picking one can select the index.
        let a = self.v[0][0];
        let b = self.v[1][0];
        let c = self.v[2][0];
        let v = a.max(b).max(c);
        if v == a {
            self.i[0]
        } else if v == b {
            self.i[1]
        } else {
            assert_eq!(v, c);
            self.i[2]
        }
    }
}

impl fmt::Display for Triangle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "T[{}:({:0.2},{:0.2}),{}:({:0.2},{:0.2}),{}:({:0.2},{:0.2})]",
            self.i[0],
            self.v[0][0],
            self.v[0][1],
            self.i[1],
            self.v[1][0],
            self.v[1][1],
            self.i[2],
            self.v[2][0],
            self.v[2][1]
        )
    }
}

impl Ord for Triangle {
    fn cmp(&self, other: &Self) -> Ordering {
        self.leftmost().cmp(&other.leftmost())
    }
}

impl PartialOrd for Triangle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Note that this has to match the PatchWinding in terrain_geo. We cannot import it from terrain_geo
// because the circular dependency would mean that any bug here or there could prevent us from
// fixing the bug.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum PatchWinding {
    Full,
    Missing0,
    Missing2,
    Missing20,
}

impl PatchWinding {
    fn has_side0(&self) -> bool {
        match self {
            Self::Full | Self::Missing2 => true,
            _ => false,
        }
    }

    fn has_side2(&self) -> bool {
        match self {
            Self::Full | Self::Missing0 => true,
            _ => false,
        }
    }

    fn all_windings() -> [Self; 4] {
        [Self::Full, Self::Missing0, Self::Missing2, Self::Missing20]
    }

    fn enum_name(&self) -> String {
        match self {
            Self::Full => "Full",
            Self::Missing0 => "Missing0",
            Self::Missing2 => "Missing2",
            Self::Missing20 => "Missing20",
        }
        .to_owned()
    }

    fn const_name(&self) -> String {
        match self {
            Self::Full => "FULL",
            Self::Missing0 => "MISSING0",
            Self::Missing2 => "MISSING2",
            Self::Missing20 => "MISSING20",
        }
        .to_owned()
    }
}

// Note: this only has to line up with whatever we do in subdivide, not patch_tree, thankfully.
fn main() -> Fallible<()> {
    let opt = Opt::from_args();

    if opt.dump_triangle_strips {
        make_triangle_strips(opt.max_level);
    }

    if opt.dump_wireframe_list {
        make_wireframe_list(opt.max_level);
    }

    if opt.dump_index_dependency_luts {
        make_index_dependency_luts(opt.max_level);
    }

    Ok(())
}

fn make_wireframe_list(max_level: usize) {
    println!("// //////////////////////////////// AUTOGENERATED DEPS //////////////////////////////// //");
    println!("// cargo run -p terrain_geo --example make-strips");
    for subdivisions in 0..=max_level {
        let (tris, _deps) = make_tris(subdivisions);
        let bins = collect_unique_rows(&tris);
        let binned = bin_by_row(&tris);
        for &winding in &PatchWinding::all_windings() {
            let indices = make_line_list(&bins, &binned, winding);
            println!(
                "pub static WIREFRAME_INDICES{}_{}: [u32; {}] = {:?};",
                subdivisions,
                winding.const_name(),
                indices.len(),
                indices
            );
        }
    }
    let mut func = "".to_owned();
    func += "use crate::patch_winding::PatchWinding;";
    func += "pub fn get_wireframe_index_buffer(subdivisions: usize, winding: PatchWinding) -> &'static [u32] {\n";
    func += "    match subdivisions {\n";
    for subdivisions in 0..=max_level {
        func += &format!("        {} => {{\n", subdivisions);
        func += "            match winding {\n";
        for &winding in &PatchWinding::all_windings() {
            func += &format!(
                "                PatchWinding::{} => &WIREFRAME_INDICES{}_{},\n",
                winding.enum_name(),
                subdivisions,
                winding.const_name()
            );
        }
        func += "            }\n";
        func += "        }\n";
    }
    func += &format!(
        "        _ => panic!(\"only up to {} subdivisions supported\")\n",
        max_level - 1
    );
    func += "    }\n}";
    println!("{}", func);
    println!("// ////////////////////////////// END AUTOGENERATED DEPS ////////////////////////////// //");
}

fn make_triangle_strips(max_level: usize) {
    println!("// //////////////////////////////// AUTOGENERATED DEPS //////////////////////////////// //");
    println!("// cargo run -p terrain_geo --example make-strips");
    for i in 0..=max_level {
        let (tris, _deps) = make_tris(i);
        let bins = collect_unique_rows(&tris);
        let binned = bin_by_row(&tris);
        // for bin in bins.iter().rev() {
        //     println!("  bin: {:?}", bin);
        //     for t in &binned[bin] {
        //         println!("    {}", t);
        //     }
        // }
        let indices = make_triangle_strip(&bins, &binned, 0);
        println!("INDICES: {:?}", indices);
    }
    println!("// ////////////////////////////// END AUTOGENERATED DEPS ////////////////////////////// //");
}

fn make_index_dependency_luts(max_level: usize) {
    println!("// //////////////////////////////// AUTOGENERATED DEPS //////////////////////////////// //");
    println!("// cargo run -p terrain_geo --example make-strips");
    for i in 0..=max_level {
        let (_tris, deps) = make_tris(i);
        println!(
            "pub static INDEX_DEPENDENCY_LUT{}: [u32; {}] = [\n    {}\n];",
            i,
            deps.len() * 2,
            deps.iter()
                .map(|e| format!("{},\n    {}", e.a, e.b))
                .collect::<Vec<_>>()
                .join(",\n    ")
        );
    }
    println!("// ////////////////////////////// END AUTOGENERATED DEPS ////////////////////////////// //");
}

fn put_tri(tri: &Triangle, indices: &mut Vec<u32>) {
    indices.push(tri.i[0]);
    indices.push(tri.i[1]);
    indices.push(tri.i[1]);
    indices.push(tri.i[2]);
    indices.push(tri.i[2]);
    indices.push(tri.i[0]);
}

fn make_line_list(
    bins: &[(i64, i64)],
    binned: &HashMap<(i64, i64), Vec<Triangle>>,
    winding: PatchWinding,
) -> Vec<u32> {
    let mut indices = Vec::new();

    if bins.len() == 1 {
        put_tri(binned[&bins[0]].first().unwrap(), &mut indices);
        return indices;
    }

    // Note that after the base case, all subdivisions result in pairs of rows.
    assert_eq!(bins.len() % 2, 0);
    assert_eq!(binned[&bins[0]].len(), 1);
    assert_eq!(binned[&bins[1]].len(), 3);

    // Handle the top point.
    {
        let tri0 = binned[&bins[0]].first().unwrap();
        let tri1 = binned[&bins[1]].first().unwrap();
        let tri2 = binned[&bins[1]].last().unwrap();
        if winding.has_side0() && winding.has_side2() {
            put_tri(tri0, &mut indices);
            put_tri(tri1, &mut indices);
            put_tri(tri2, &mut indices);
        } else if winding.has_side0() && !winding.has_side2() {
            indices.push(tri0.i[0]);
            indices.push(tri2.i[2]);
            indices.push(tri2.i[2]);
            indices.push(tri2.i[1]);
            indices.push(tri2.i[1]);
            indices.push(tri0.i[0]);
            put_tri(tri1, &mut indices);
            indices.push(tri0.i[0]);
            indices.push(tri1.i[0]);
        } else if !winding.has_side0() && winding.has_side2() {
            indices.push(tri0.i[0]);
            indices.push(tri1.i[1]);
            indices.push(tri1.i[1]);
            indices.push(tri1.i[2]);
            indices.push(tri1.i[2]);
            indices.push(tri0.i[0]);
            put_tri(tri2, &mut indices);
            indices.push(tri0.i[0]);
            indices.push(tri2.i[0]);
        } else {
            indices.push(tri0.i[0]);
            indices.push(tri1.i[1]);
            indices.push(tri1.i[1]);
            indices.push(tri2.i[2]);
            indices.push(tri2.i[2]);
            indices.push(tri0.i[0]);
            indices.push(tri0.i[0]);
            indices.push(tri1.i[2]);
        }
    }

    for pairs in bins.chunks(2).skip(1) {
        let bin_a = pairs[0];
        let bin_b = pairs[1];

        let tri_a = binned[&bin_a].first().unwrap();
        let tri_b = binned[&bin_b].first().unwrap();
        if winding.has_side0() {
            put_tri(tri_a, &mut indices);
            put_tri(tri_b, &mut indices);
        } else {
            indices.push(tri_a.i[0]);
            indices.push(tri_b.i[1]);
            indices.push(tri_b.i[1]);
            indices.push(tri_a.i[2]);
            indices.push(tri_a.i[2]);
            indices.push(tri_a.i[0]);
            indices.push(tri_b.i[1]);
            indices.push(tri_b.i[2]);
        }

        for (i, tri) in binned[&bin_a]
            .iter()
            .enumerate()
            .skip(1)
            .take(binned[&bin_a].len() - 3)
        {
            if i % 2 == 0 {
                // upward facing tris
                // Push bottom
                indices.push(tri.i[1]);
                indices.push(tri.i[2]);
                // Push right
                indices.push(tri.i[2]);
                indices.push(tri.i[0]);
            } else {
                // downward facing tris have their inverted bottom pushed by prior row.
                // Push right
                indices.push(tri.i[0]);
                indices.push(tri.i[1]);
            }
        }

        for (i, tri) in binned[&bin_b]
            .iter()
            .enumerate()
            .skip(1)
            .take(binned[&bin_b].len() - 3)
        {
            if i % 2 == 0 {
                // upward facing tris
                // Push bottom
                indices.push(tri.i[1]);
                indices.push(tri.i[2]);
                // Push right
                indices.push(tri.i[2]);
                indices.push(tri.i[0]);
            } else {
                // downward facing tris have their inverted bottom pushed by prior row.
                // Push right
                indices.push(tri.i[0]);
                indices.push(tri.i[1]);
            }
        }

        let tri_a = binned[&bin_a].last().unwrap();
        let tri_b = binned[&bin_b].last().unwrap();
        if winding.has_side2() {
            put_tri(tri_a, &mut indices);
            put_tri(tri_b, &mut indices);
        } else {
            indices.push(tri_a.i[0]);
            indices.push(tri_a.i[1]);
            indices.push(tri_a.i[1]);
            indices.push(tri_b.i[2]);
            indices.push(tri_b.i[2]);
            indices.push(tri_a.i[0]);
            indices.push(tri_b.i[2]);
            indices.push(tri_b.i[1]);
        }
    }

    indices
}

fn make_triangle_strip(
    bins: &[(i64, i64)],
    binned: &HashMap<(i64, i64), Vec<Triangle>>,
    _winding: u8,
) -> Vec<u32> {
    let mut indices = Vec::new();

    for bin in bins.iter().rev() {
        println!(
            "BIN: {:?}: {:?}",
            bin,
            binned[bin].iter().map(|t| t.i).collect::<Vec<_>>()
        );
        // Start off each row with the left two verts after resetting.
        let fst = binned[bin].first().unwrap();
        indices.push(fst.i[0]);
        indices.push(fst.i[1]);
        for tri in &binned[bin] {
            indices.push(tri.rightmost_index());
        }
    }

    indices
}

fn bin_by_row(tris: &[Triangle]) -> HashMap<(i64, i64), Vec<Triangle>> {
    let row_bins = collect_unique_rows(tris).drain(..).collect::<HashSet<_>>();
    let mut bins: HashMap<(i64, i64), Vec<Triangle>> = HashMap::new();
    for t in tris {
        let bin = row_bin_for_tri(t);
        assert!(row_bins.contains(&bin));
        bins.entry(bin)
            .and_modify(|v| v.push(*t))
            .or_insert_with(|| vec![*t]);
    }

    // Sort bins by leftmost coordinate.
    for tris in bins.values_mut() {
        tris.sort();
    }

    bins
}

fn row_bin_for_tri(tri: &Triangle) -> (i64, i64) {
    let a = tri.v[0][1];
    let b = tri.v[1][1];
    let c = tri.v[2][1];
    if a == c {
        assert_ne!(b, a);
        assert_ne!(b, c);
        if a < b {
            (a, b)
        } else {
            (b, a)
        }
    } else if b == c {
        assert_ne!(a, b);
        assert_ne!(a, c);
        if a < c {
            (a, c)
        } else {
            (c, a)
        }
    } else {
        assert_eq!(a, b);
        assert_ne!(c, a);
        assert_ne!(c, b);
        if a < c {
            (a, c)
        } else {
            (c, a)
        }
    }
}

fn collect_unique_rows(tris: &[Triangle]) -> Vec<(i64, i64)> {
    let mut uniq = HashSet::new();
    for t in tris {
        uniq.insert(t.v[0][1]);
        uniq.insert(t.v[1][1]);
        uniq.insert(t.v[2][1]);
    }
    let mut v = uniq.drain().collect::<Vec<i64>>();
    v.sort();
    let mut bins = (&v).windows(2).map(|v| (v[0], v[1])).collect::<Vec<_>>();
    bins.reverse();
    bins
}

fn count_unique_vertices(tris: &[Triangle]) -> usize {
    let mut uniq = HashSet::new();
    for t in tris {
        uniq.insert(t.v[0]);
        uniq.insert(t.v[1]);
        uniq.insert(t.v[2]);
    }
    uniq.len()
}

fn make_tris(subdivisions: usize) -> (Vec<Triangle>, Vec<Edge>) {
    let s = 100 * (subdivisions + 1) as i64;
    let v0 = Vector2::new(0, s);
    let v1 = Vector2::new(-s, -s);
    let v2 = Vector2::new(s, -s);
    let tri = Triangle::new(0, v0, 1, v1, 2, v2);
    let mut indices = HashMap::new();
    indices.insert(v0, (0, Edge::new(u32::MAX, u32::MAX)));
    indices.insert(v1, (1, Edge::new(u32::MAX, u32::MAX)));
    indices.insert(v2, (2, Edge::new(u32::MAX, u32::MAX)));
    let mut deps_lut = HashMap::new();
    deps_lut.insert(0, Edge::new(u32::MAX, u32::MAX));
    deps_lut.insert(1, Edge::new(u32::MAX, u32::MAX));
    deps_lut.insert(2, Edge::new(u32::MAX, u32::MAX));
    let mut tris = vec![];
    for target in 0..=subdivisions {
        subdivide_tris_inner(
            target,
            0,
            subdivisions,
            tri,
            &mut tris,
            &mut indices,
            &mut deps_lut,
        );
    }

    assert_eq!(deps_lut.len(), count_unique_vertices(&tris));
    let mut deps = Vec::new();
    for i in 0u32..deps_lut.len() as u32 {
        deps.push(deps_lut[&i]);
    }

    (tris, deps)
}

fn get_index(
    active: bool,
    v: &Vector2<i64>,
    edge: Edge,
    indices: &mut HashMap<Vector2<i64>, (u32, Edge)>,
    deps_lut: &mut HashMap<u32, Edge>,
) -> u32 {
    if let Some(&(index, existing_edge)) = indices.get(v) {
        assert_eq!(edge, existing_edge);
        return index;
    }
    let index = indices.len() as u32;
    if active {
        indices.insert(*v, (index, edge));
        deps_lut.insert(index, edge);
    }
    index
}

fn subdivide_tris_inner(
    target_level: usize,
    level: usize,
    max_level: usize,
    tri: Triangle,
    tris: &mut Vec<Triangle>,
    indices: &mut HashMap<Vector2<i64>, (u32, Edge)>,
    deps_lut: &mut HashMap<u32, Edge>,
) {
    let active = level == target_level;

    if level >= max_level {
        if active {
            tris.push(tri);
        }
        return;
    }

    // Subdivide tri and head down.
    let v0 = tri.v[0];
    let v1 = tri.v[1];
    let v2 = tri.v[2];
    let i0 = tri.i[0];
    let i1 = tri.i[1];
    let i2 = tri.i[2];
    let a = (v0 + v1) / 2;
    let ia = get_index(active, &a, Edge::new(i0, i1), indices, deps_lut);
    let b = (v1 + v2) / 2;
    let ib = get_index(active, &b, Edge::new(i1, i2), indices, deps_lut);
    let c = (v2 + v0) / 2;
    let ic = get_index(active, &c, Edge::new(i2, i0), indices, deps_lut);
    subdivide_tris_inner(
        target_level,
        level + 1,
        max_level,
        Triangle::new(i0, v0, ia, a, ic, c),
        tris,
        indices,
        deps_lut,
    );
    subdivide_tris_inner(
        target_level,
        level + 1,
        max_level,
        Triangle::new(ia, a, i1, v1, ib, b),
        tris,
        indices,
        deps_lut,
    );
    subdivide_tris_inner(
        target_level,
        level + 1,
        max_level,
        Triangle::new(ic, c, ib, b, i2, v2),
        tris,
        indices,
        deps_lut,
    );
    subdivide_tris_inner(
        target_level,
        level + 1,
        max_level,
        Triangle::new(ib, b, ic, c, ia, a),
        tris,
        indices,
        deps_lut,
    );
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_uniq_verts() {
        let mut all_deps = Vec::new();
        for i in 0..7 {
            let (_tris, deps) = make_tris(i);
            all_deps.push(deps);
        }
        for pair in all_deps.windows(2) {
            let a = &pair[0];
            let b = &pair[1];
            for (i, e) in a.iter().enumerate() {
                assert_eq!(*e, b[i]);
            }
        }

        for i in 0..8 {
            let (tris, _) = make_tris(i);
            let expect = (((2f64.powf(i as f64) + 1f64) * (2f64.powf(i as f64) + 2f64)) / 2f64)
                .floor() as usize;
            assert_eq!(count_unique_vertices(&tris), expect);
            let bins = collect_unique_rows(&tris);
            let binned = bin_by_row(&tris);
            for &winding in &PatchWinding::all_windings() {
                make_line_list(&bins, &binned, winding);
            }
        }
    }
}
