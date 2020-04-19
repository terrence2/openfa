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
use crate::patch::Patch;

use absolute_unit::Kilometers;
use camera::ArcBallCamera;
use failure::Fallible;
use geometry::{IcoSphere, Plane};
use nalgebra::{Point3, Vector3};
use std::time::Instant;
use universe::EARTH_RADIUS_KM;

// Index into the tree vec.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct TreeIndex(usize);

// Index into the patch vec.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct PatchIndex(pub(crate) usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TreeNode {
    Root { children: [TreeIndex; 20] },
    Node { children: [TreeIndex; 4] },
    Leaf { offset: PatchIndex },
    Empty,
}

impl TreeNode {
    // Panic if this is not a leaf.
    fn leaf_offset(&self) -> PatchIndex {
        match self {
            Self::Leaf { offset } => return *offset,
            _ => panic!("Not a leaf!"),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            _ => false,
        }
    }
}

pub(crate) struct PatchTree {
    sphere: IcoSphere,
    depth_levels: Vec<f64>,
    patches: Vec<Patch>,
    patch_order: Vec<usize>,
    patch_tree: Vec<TreeNode>,
    num_patches: usize,
}

impl PatchTree {
    pub(crate) fn new(num_patches: usize) -> Self {
        const LEVEL_COUNT: usize = 40;
        let mut depth_levels = Vec::new();
        for i in 0..LEVEL_COUNT {
            depth_levels.push(EARTH_RADIUS_KM * 2f64.powf(-(i as f64)));
        }
        for lvl in depth_levels.iter_mut() {
            *lvl = *lvl * *lvl;
        }
        println!("depths: {:?}", depth_levels);

        let sphere = IcoSphere::new(0);
        let mut patches = Vec::with_capacity(num_patches);
        patches.resize(num_patches, Patch::new());
        let mut patch_order = Vec::with_capacity(num_patches);
        for i in 0..num_patches {
            patch_order.push(i);
        }
        let mut patch_tree = Vec::new();
        patch_tree.push(TreeNode::Root {
            children: [
                TreeIndex(1),
                TreeIndex(2),
                TreeIndex(3),
                TreeIndex(4),
                TreeIndex(5),
                TreeIndex(6),
                TreeIndex(7),
                TreeIndex(8),
                TreeIndex(9),
                TreeIndex(10),
                TreeIndex(11),
                TreeIndex(12),
                TreeIndex(13),
                TreeIndex(14),
                TreeIndex(15),
                TreeIndex(16),
                TreeIndex(17),
                TreeIndex(18),
                TreeIndex(19),
                TreeIndex(20),
            ],
        });
        for (i, face) in sphere.faces.iter().enumerate() {
            let v0 = Point3::from(&sphere.verts[face.i0()] * EARTH_RADIUS_KM);
            let v1 = Point3::from(&sphere.verts[face.i1()] * EARTH_RADIUS_KM);
            let v2 = Point3::from(&sphere.verts[face.i2()] * EARTH_RADIUS_KM);
            patches[i].change_target(0, [v0, v1, v2]);
            patch_tree.push(TreeNode::Leaf {
                offset: PatchIndex(i),
            });
        }

        Self {
            sphere,
            depth_levels,
            patches,
            patch_order,
            patch_tree,
            num_patches,
        }
    }

    pub(crate) fn get_patch(&self, index: PatchIndex) -> &Patch {
        &self.patches[index.0]
    }

    fn get_patch_mut(&mut self, index: PatchIndex) -> &mut Patch {
        &mut self.patches[index.0]
    }

    fn tree_root(&self) -> TreeNode {
        self.patch_tree[0]
    }

    fn tree_node(&self, index: TreeIndex) -> TreeNode {
        self.patch_tree[index.0]
    }

    fn set_tree_node(&mut self, index: TreeIndex, node: TreeNode) {
        self.patch_tree[index.0] = node;
    }

    fn clear_tree_node(&mut self, index: TreeIndex) {
        self.patch_tree[index.0] = TreeNode::Empty;
    }

    fn format_tree_display(&self) -> String {
        self.format_tree_display_inner(0, self.tree_root())
    }

    fn format_tree_display_inner(&self, lvl: usize, node: TreeNode) -> String {
        let mut out = String::new();
        match node {
            TreeNode::Root { children } => {
                out += "Root\n";
                for child in &children {
                    out += &self.format_tree_display_inner(lvl + 1, self.tree_node(*child));
                }
            }
            TreeNode::Node { children } => {
                let pad = "  ".repeat(lvl);
                out += &format!("{}Node: {:?}\n", pad, children);
                for child in &children {
                    out += &self.format_tree_display_inner(lvl + 1, self.tree_node(*child));
                }
            }
            TreeNode::Leaf { offset } => {
                let pad = "  ".repeat(lvl);
                out += &format!("{}Leaf @{}, lvl: {}\n", pad, offset.0, lvl);
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
        return out;
    }

    pub(crate) fn optimize_for_view(&mut self, camera: &ArcBallCamera) {
        let camera_target = camera.cartesian_target_position::<Kilometers>().vec64();
        let eye_position = camera.cartesian_eye_position::<Kilometers>().point64();
        let eye_direction = camera_target - eye_position.coords;

        let horizon_plane = Plane::from_normal_and_distance(
            eye_position.coords.normalize(),
            (((EARTH_RADIUS_KM * EARTH_RADIUS_KM) / eye_position.coords.magnitude()) - 100f64)
                .min(0f64),
        );

        /*
        let loop_start = Instant::now();
        let mut patches = BinaryHeap::with_capacity(self.num_patches);
        for face in &self.sphere.faces {
            let v0 = Point3::from(self.sphere.verts[face.i0()] * EARTH_RADIUS_KM);
            let v1 = Point3::from(self.sphere.verts[face.i1()] * EARTH_RADIUS_KM);
            let v2 = Point3::from(self.sphere.verts[face.i2()] * EARTH_RADIUS_KM);
            let patch = PatchInfo::new(0, &eye_position, &eye_direction, [v0, v1, v2]);

            //println!("Checking {}: ", i);
            if patch.keep(camera, &horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch);
            }
        }
        let elapsed = Instant::now() - loop_start;
        println!(
            "lvl0: {:?}, {:?}us per iteration - {} patches",
            elapsed,
            elapsed.as_micros() / self.sphere.faces.len() as u128,
            patches.len(),
        );
        */

        // Split patches until we have an optimal equal-area partitioning.
        /*
        let loop_start = Instant::now();
        while patches.len() > 0 && patches.len() < self.num_patches - 4 {
            let patch = patches.pop().unwrap();
            let [v0, v1, v2] = patch.pts;
            let a = Point3::from(
                IcoSphere::bisect_edge(&v0.coords, &v1.coords).normalize() * EARTH_RADIUS_KM,
            );
            let b = Point3::from(
                IcoSphere::bisect_edge(&v1.coords, &v2.coords).normalize() * EARTH_RADIUS_KM,
            );
            let c = Point3::from(
                IcoSphere::bisect_edge(&v2.coords, &v0.coords).normalize() * EARTH_RADIUS_KM,
            );

            let patch0 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [v0, a, c]);
            let patch1 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [v1, b, a]);
            let patch2 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [v2, c, b]);
            let patch3 = PatchInfo::new(patch.level + 1, &eye_position, &eye_direction, [a, b, c]);

            if patch0.keep(camera, &horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch0);
            }
            if patch1.keep(camera, &horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch1);
            }
            if patch2.keep(camera, &horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch2);
            }
            if patch3.keep(camera, &horizon_plane, &eye_direction, &eye_position) {
                patches.push(patch3);
            }
        }
        println!("split: {:?}", Instant::now() - loop_start);
        */

        /*
        // Recompute solid angles for all patches.
        let recompute_sa_start = Instant::now();
        for patch in self.patches.iter_mut() {
            if patch.keep(camera, &horizon_plane, &eye_direction, &eye_position) {
                patch.recompute_solid_angle(&eye_position, &eye_direction);
            } else {
                patch.erect_tombstone();
            }
        }
        let recompute_sa_end = Instant::now();
        println!("solid ang: {:?}", recompute_sa_end - recompute_sa_start);

        // Sort by solid angle, using an indirection buffer so we can avoid a bunch of copying.
        let sort_sa_start = Instant::now();
        // Note, we still have to do this extra copy, because rust. The borrow of
        // self would be safe, but we lose that info across the closure.
        let mut order = self.patch_order.clone();
        order.sort_unstable_by(|patch_a_index, patch_b_index| {
            let a = self.patches[*patch_a_index].solid_angle;
            let b = self.patches[*patch_b_index].solid_angle;
            if a < b {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });
        // Note: no point writing back -- we're not using stable sort or something.
        let sort_sa_end = Instant::now();
        println!("sort solid ang: {:?}", sort_sa_end - sort_sa_start);
        */

        let rejoin_start = Instant::now();
        self.rejoin_tree_to_depth(&camera, &horizon_plane, &eye_position, TreeIndex(0));
        let rejoin_end = Instant::now();
        println!("rejoin: {:?}", rejoin_end - rejoin_start);

        let subdivide_start = Instant::now();
        self.subdivide_tree_to_depth(
            &camera,
            &horizon_plane,
            &eye_position,
            &eye_direction,
            self.tree_root(),
        );
        let subdivide_end = Instant::now();
        println!("subdivide: {:?}", subdivide_end - subdivide_start);
    }

    fn rejoin_tree_to_depth(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        node_index: TreeIndex,
    ) {
        match self.tree_node(node_index) {
            TreeNode::Root { ref children } => {
                for i in children {
                    self.rejoin_tree_to_depth(camera, horizon_plane, eye_position, *i);
                }
            }
            TreeNode::Node { ref children } => {
                for i in children {
                    self.rejoin_tree_to_depth(camera, horizon_plane, eye_position, *i);
                }
                if children.iter().all(|child| {
                    self.leaf_can_be_rejoined(
                        camera,
                        horizon_plane,
                        eye_position,
                        self.tree_node(*child),
                    )
                }) {
                    let new_child = self.rejoin_patch(children);
                    self.set_tree_node(node_index, new_child);
                }
            }
            TreeNode::Leaf { offset } => {}
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn rejoin_patch(&mut self, children: &[TreeIndex; 4]) -> TreeNode {
        let i0 = self.tree_node(children[0]).leaf_offset();
        let i1 = self.tree_node(children[1]).leaf_offset();
        let i2 = self.tree_node(children[2]).leaf_offset();
        let i3 = self.tree_node(children[3]).leaf_offset();
        let lvl = self.get_patch(i0).level() as u16 - 1;
        let v0 = *self.get_patch(i0).point(0);
        let v1 = *self.get_patch(i1).point(0);
        let v2 = *self.get_patch(i2).point(0);
        self.get_patch_mut(i0).change_target(lvl, [v0, v1, v2]);
        self.get_patch_mut(i1).erect_tombstone();
        self.get_patch_mut(i2).erect_tombstone();
        self.get_patch_mut(i3).erect_tombstone();
        self.clear_tree_node(children[1]);
        self.clear_tree_node(children[2]);
        self.clear_tree_node(children[3]);
        TreeNode::Leaf { offset: i0 }
    }

    fn leaf_can_be_rejoined(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        node: TreeNode,
    ) -> bool {
        match node {
            TreeNode::Root { .. } => false,
            TreeNode::Node { .. } => false,
            TreeNode::Leaf { offset } => {
                let patch = self.get_patch(offset);
                let d2 = patch.distance_squared_to(eye_position);
                assert!(patch.level() > 0);
                d2 > self.depth_levels[patch.level() - 1]
                    || !patch.keep(camera, horizon_plane, eye_position)
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn subdivide_tree_to_depth(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        eye_direction: &Vector3<f64>,
        node: TreeNode,
    ) {
        match node {
            TreeNode::Root { ref children } => {
                for i in children {
                    if let Some(new_child) = self.maybe_subdivide_patch(
                        camera,
                        horizon_plane,
                        eye_position,
                        eye_direction,
                        self.tree_node(*i),
                        false,
                    ) {
                        println!("SUBDIVIDE ROOT: {:?}", *i);
                        self.set_tree_node(*i, new_child);
                    }
                }
                for i in children {
                    self.subdivide_tree_to_depth(
                        camera,
                        horizon_plane,
                        eye_position,
                        eye_direction,
                        self.tree_node(*i),
                    );
                }
            }
            TreeNode::Node { ref children } => {
                for i in children {
                    if let Some(new_child) = self.maybe_subdivide_patch(
                        camera,
                        horizon_plane,
                        eye_position,
                        eye_direction,
                        self.tree_node(*i),
                        false,
                    ) {
                        println!("SUBDIVIDE NODE");
                        self.set_tree_node(*i, new_child);
                    }
                }
                for i in children {
                    self.subdivide_tree_to_depth(
                        camera,
                        horizon_plane,
                        eye_position,
                        &eye_direction,
                        self.tree_node(*i),
                    );
                }
            }
            TreeNode::Leaf { offset } => {}
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn maybe_subdivide_patch(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        eye_direction: &Vector3<f64>,
        node: TreeNode,
        force: bool,
    ) -> Option<TreeNode> {
        match node {
            TreeNode::Root { .. } => None,
            TreeNode::Node { .. } => None,
            TreeNode::Leaf { offset } => {
                let (maybe_offsets, patch_pts, patch_level) = {
                    let patch = self.get_patch(offset);
                    let d2 = patch.distance_squared_to(eye_position);
                    if (d2 > self.depth_levels[patch.level()]
                        || !patch.keep(camera, horizon_plane, eye_position))
                        && !force
                    {
                        return None;
                    }

                    let maybe_offsets = self.find_empty_patch_slots();
                    if maybe_offsets.is_none() {
                        // No room for new patches, so skip it.
                        println!("    OUT OF ROOM IN PATCHES");
                        return None;
                    }

                    (
                        maybe_offsets,
                        patch.points().to_owned(),
                        patch.level() as u16,
                    )
                };

                let [v0, v1, v2] = patch_pts;
                let a = Point3::from(
                    IcoSphere::bisect_edge(&v0.coords, &v1.coords).normalize() * EARTH_RADIUS_KM,
                );
                let b = Point3::from(
                    IcoSphere::bisect_edge(&v1.coords, &v2.coords).normalize() * EARTH_RADIUS_KM,
                );
                let c = Point3::from(
                    IcoSphere::bisect_edge(&v2.coords, &v0.coords).normalize() * EARTH_RADIUS_KM,
                );
                let [p0off, p1off, p2off, p3off] = maybe_offsets.unwrap();
                self.get_patch_mut(p0off)
                    .change_target(patch_level + 1, [v0, a, c]);
                self.get_patch_mut(p1off)
                    .change_target(patch_level + 1, [v1, b, a]);
                self.get_patch_mut(p2off)
                    .change_target(patch_level + 1, [v2, c, b]);
                self.get_patch_mut(p3off)
                    .change_target(patch_level + 1, [a, b, c]);
                self.get_patch_mut(offset).erect_tombstone();

                let [pt0off, pt1off, pt2off, pt3off] = self.find_empty_tree_slots();
                self.set_tree_node(pt0off, TreeNode::Leaf { offset: p0off });
                self.set_tree_node(pt1off, TreeNode::Leaf { offset: p1off });
                self.set_tree_node(pt2off, TreeNode::Leaf { offset: p2off });
                self.set_tree_node(pt3off, TreeNode::Leaf { offset: p3off });

                return Some(TreeNode::Node {
                    children: [pt0off, pt1off, pt2off, pt3off],
                });
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn find_empty_patch_slots(&self) -> Option<[PatchIndex; 4]> {
        let mut out = [PatchIndex(0); 4];
        let mut offset = 0;
        for (i, p) in self.patches.iter().enumerate() {
            if !p.is_alive() {
                out[offset] = PatchIndex(i);
                offset += 1;
                if offset > 3 {
                    return Some(out);
                }
            }
        }
        None
    }

    fn find_empty_tree_slots(&mut self) -> [TreeIndex; 4] {
        let mut out = [TreeIndex(0); 4];
        let mut offset = 0;
        for (i, p) in self.patch_tree.iter().enumerate() {
            if p.is_empty() {
                out[offset] = TreeIndex(i);
                offset += 1;
                if offset > 3 {
                    return out;
                }
            }
        }
        while offset < 4 {
            out[offset] = TreeIndex(self.patch_tree.len());
            offset += 1;
            self.patch_tree.push(TreeNode::Empty);
        }
        out
    }

    pub fn num_patches(&self) -> usize {
        self.num_patches
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_levels() {}
}
