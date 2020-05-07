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
use crate::{debug_vertex::DebugVertex, patch::Patch};

use absolute_unit::Kilometers;
use camera::ArcBallCamera;
use failure::Fallible;
use failure::_core::fmt::Binary;
use geometry::{IcoSphere, Plane};
use nalgebra::{Point3, Vector3};
use physical_constants::EARTH_RADIUS_KM;
use std::{
    cmp::{Ordering, Reverse},
    collections::{BinaryHeap, HashSet},
    time::Instant,
};

// Index into the tree vec.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TreeIndex(usize);

// Index into the patch vec.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct PatchIndex(pub(crate) usize);

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Root {
    children: [Option<TreeIndex>; 20],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Node {
    children: [TreeIndex; 4],
    parent: TreeIndex,
    patch_index: PatchIndex,
    level: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Leaf {
    patch_index: PatchIndex,
    parent: TreeIndex,
    level: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TreeNode {
    Root,
    Node(Node),
    Leaf(Leaf),
    Empty,
}

impl TreeNode {
    fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            _ => false,
        }
    }

    fn is_leaf(&self) -> bool {
        match self {
            Self::Leaf(_) => true,
            _ => false,
        }
    }

    fn is_node(&self) -> bool {
        match self {
            Self::Node(_) => true,
            _ => false,
        }
    }

    fn as_leaf_mut(&mut self) -> &mut Leaf {
        match self {
            Self::Leaf(leaf) => leaf,
            _ => panic!("Not a leaf tree node!"),
        }
    }

    fn as_node(&self) -> &Node {
        match self {
            Self::Node(ref node) => node,
            _ => panic!("Not a node tree node!"),
        }
    }

    fn parent(&self) -> TreeIndex {
        match self {
            Self::Leaf(leaf) => leaf.parent,
            Self::Node(node) => node.parent,
            _ => panic!("Node type does not have a parent!"),
        }
    }

    fn level(&self) -> usize {
        match self {
            Self::Leaf(leaf) => leaf.level,
            Self::Node(node) => node.level,
            _ => panic!("Node type does not have a level!"),
        }
    }

    // Panic if this is not a leaf.
    fn patch_index(&self) -> PatchIndex {
        match self {
            Self::Leaf(ref leaf) => return leaf.patch_index,
            Self::Node(ref node) => return node.patch_index,
            _ => panic!("Not a leaf!"),
        }
    }
}

const MAX_LEVEL: usize = 15;

pub(crate) struct PatchTree {
    num_patches: usize,
    sphere: IcoSphere,
    depth_levels: Vec<f64>,
    patches: Vec<Patch>,
    patch_empty_set: BinaryHeap<Reverse<PatchIndex>>,
    tree: Vec<TreeNode>,
    tree_empty_set: BinaryHeap<Reverse<TreeIndex>>,
    root: Root,
    root_patches: [Patch; 20],

    subdivide_count: usize,
    rejoin_count: usize,
    visit_count: usize,

    cached_viewable_region: [Plane<f64>; 6],
    cached_eye_position: Point3<f64>,
    cached_eye_direction: Vector3<f64>,
}

impl PatchTree {
    pub(crate) fn new(num_patches: usize) -> Self {
        let mut depth_levels = Vec::new();
        for i in 0..=MAX_LEVEL {
            let d = 1f64 * EARTH_RADIUS_KM * 2f64.powf(-(i as f64 * 0.8));
            depth_levels.push(d * d);
        }

        let sphere = IcoSphere::new(0);
        let mut patches = Vec::with_capacity(num_patches);
        let mut tree = Vec::with_capacity(num_patches);
        let root = Root {
            children: [None; 20],
        };
        tree.push(TreeNode::Root);
        let cached_eye_position = Point3::new(0f64, 0f64, 0f64);
        let cached_eye_direction = Vector3::new(1f64, 0f64, 0f64);
        let cached_viewable_region =
            [Plane::from_normal_and_distance(Vector3::new(1f64, 0f64, 0f64), 0f64); 6];
        let mut root_patches = [Patch::new(); 20];
        for (i, face) in sphere.faces.iter().enumerate() {
            let v0 = Point3::from(&sphere.verts[face.i0()] * EARTH_RADIUS_KM);
            let v1 = Point3::from(&sphere.verts[face.i1()] * EARTH_RADIUS_KM);
            let v2 = Point3::from(&sphere.verts[face.i2()] * EARTH_RADIUS_KM);
            root_patches[i].change_target(TreeIndex(0), [v0, v1, v2]);
        }

        Self {
            num_patches,
            sphere,
            depth_levels,
            patches,
            patch_empty_set: BinaryHeap::new(),
            tree,
            tree_empty_set: BinaryHeap::new(),
            root,
            root_patches,
            subdivide_count: 0,
            rejoin_count: 0,
            visit_count: 0,
            cached_viewable_region,
            cached_eye_position,
            cached_eye_direction,
        }
    }

    pub(crate) fn get_patch(&self, index: PatchIndex) -> &Patch {
        &self.patches[index.0]
    }

    fn get_patch_mut(&mut self, index: PatchIndex) -> &mut Patch {
        &mut self.patches[index.0]
    }

    pub(crate) fn level_of_patch(&self, patch_index: PatchIndex) -> usize {
        self.tree_node(self.get_patch(patch_index).owner()).level()
    }

    fn allocate_patch(&mut self) -> PatchIndex {
        if let Some(Reverse(patch_index)) = self.patch_empty_set.pop() {
            return patch_index;
        }
        let patch_index = PatchIndex(self.patches.len());
        self.patches.push(Patch::new());
        return patch_index;
    }

    fn free_patch(&mut self, patch_index: PatchIndex) {
        self.get_patch_mut(patch_index).erect_tombstone();
        self.patch_empty_set.push(Reverse(patch_index));
    }

    fn patch_count(&self) -> usize {
        self.patches.len() - self.patch_empty_set.len()
    }

    fn allocate_tree_node(&mut self) -> TreeIndex {
        if let Some(Reverse(tree_index)) = self.tree_empty_set.pop() {
            return tree_index;
        }
        let tree_index = TreeIndex(self.tree.len());
        self.tree.push(TreeNode::Empty);
        return tree_index;
    }

    fn free_tree_node(&mut self, tree_index: TreeIndex) {
        self.set_tree_node(tree_index, TreeNode::Empty);
        self.tree_empty_set.push(Reverse(tree_index));
    }

    fn allocate_leaf(
        &mut self,
        parent: TreeIndex,
        level: usize,
        pts: [Point3<f64>; 3],
    ) -> TreeIndex {
        let patch_index = self.allocate_patch();
        let tree_index = self.allocate_tree_node();
        self.get_patch_mut(patch_index)
            .change_target(tree_index, pts);
        self.set_tree_node(
            tree_index,
            TreeNode::Leaf(Leaf {
                parent,
                level,
                patch_index,
            }),
        );
        return tree_index;
    }

    fn free_leaf(&mut self, leaf_index: TreeIndex) {
        assert!(self.tree_node(leaf_index).is_leaf(), "trying to remove root patch that is not a leaf! How did we get over the horizon while still being close enough to be subdivided?");
        self.free_patch(self.tree_node(leaf_index).patch_index());
        self.free_tree_node(leaf_index);
    }

    /*
    // Ensure that patches are linear by doing tail-swap removals from the empty patch list.
    fn compact_patches(&mut self) {
        while let Some(empty_index) = self.patch_empty_set.pop() {
            let mut last_index = PatchIndex(self.patches.len() - 1);
            while self.patches[last_index.0].is_tombstone() {
                let _ = self.patches.pop().unwrap();
                last_index = PatchIndex(self.patches.len() - 1);
            }
            if empty_index.0 >= self.patches.len() {
                continue;
            }
            if empty_index != last_index {
                self.patches[empty_index.0] = self.patches[last_index.0];
                self.tree[self.patches[empty_index.0].owner().0]
                    .as_leaf_mut()
                    .patch_index = empty_index;
            }
            let _ = self.patches.pop().unwrap();
        }
    }

    fn order_patches(&mut self) {
        self.patches.sort_by(|a, b| {
            assert!(a.is_alive());
            assert!(b.is_alive());
            if a > b {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });
        // TODO: measure to see if an indirection buffer is faster than this fixup + the extra overhead of copying patches around.
        for (i, patch) in self.patches.iter().enumerate() {
            self.tree[patch.owner().0].as_leaf_mut().patch_index = PatchIndex(i);
        }
    }
     */

    fn tree_root(&self) -> TreeNode {
        self.tree[0]
    }

    fn tree_node(&self, index: TreeIndex) -> TreeNode {
        self.tree[index.0]
    }

    fn set_tree_node(&mut self, index: TreeIndex, node: TreeNode) {
        self.tree[index.0] = node;
    }

    pub(crate) fn optimize_for_view(
        &mut self,
        camera: &ArcBallCamera,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        self.subdivide_count = 0;
        self.rejoin_count = 0;
        self.visit_count = 0;

        let camera_target = camera.cartesian_target_position::<Kilometers>().vec64();
        let eye_position = camera.cartesian_eye_position::<Kilometers>().point64();
        let eye_direction = camera_target - eye_position.coords;
        self.cached_eye_position = eye_position;
        self.cached_eye_direction = eye_direction;

        for (i, f) in camera.world_space_frustum().iter().enumerate() {
            self.cached_viewable_region[i] = *f;
        }
        self.cached_viewable_region[5] = Plane::from_normal_and_distance(
            eye_position.coords.normalize(),
            (((EARTH_RADIUS_KM * EARTH_RADIUS_KM) / eye_position.coords.magnitude()) - 100f64)
                .min(0f64),
        );

        // Make sure we have the right root set.
        let root_vis_start = Instant::now();
        self.ensure_root_visibility();
        //self.compact_patches();
        let root_vis_time = Instant::now() - root_vis_start;

        // Build a view-direction independent tesselation based on the current camera position.
        let reshape_start = Instant::now();
        self.apply_distance_function(&eye_position, live_patches);
        let reshape_time = Instant::now() - reshape_start;

        // Select patches based on visibility.
        println!(
            "patches: {} of {} [-{}] | nodes: {} [-{}] | -/+: {}/{}/{} | {:?} + {:?}",
            live_patches.len(),
            self.patches.len(),
            self.patch_empty_set.len(),
            self.tree.len(),
            self.tree_empty_set.len(),
            self.rejoin_count,
            self.subdivide_count,
            self.visit_count,
            root_vis_time,
            reshape_time,
        );
    }

    fn ensure_root_visibility(&mut self) {
        for i in 0..20 {
            if self.root_patches[i].keep(&self.cached_viewable_region, &self.cached_eye_position) {
                if self.root.children[i].is_none() {
                    let pts = self.root_patches[i].points().to_owned();
                    let leaf_index = self.allocate_leaf(TreeIndex(0), 1, pts);
                    self.root.children[i] = Some(leaf_index);
                }
            } else {
                if let Some(tree_index) = self.root.children[i] {
                    // Remove newly invisible root patch from patch tree.
                    self.collapse_node_to_leaf(tree_index);
                    self.free_leaf(tree_index);
                    self.root.children[i] = None;
                }
            }
        }
    }

    fn collapse_node_to_leaf(&mut self, tree_index: TreeIndex) {
        //println!("collapsing: {:?}", self.tree_node(tree_index));
        match self.tree_node(tree_index) {
            TreeNode::Node(ref node) => {
                for i in &node.children {
                    self.collapse_node_to_leaf(*i);
                }
                assert!(self.is_leaf_patch(node));
                self.rejoin_leaf_patch_into(node.parent, node.level, tree_index, &node.children);
            }
            TreeNode::Root => {}
            TreeNode::Leaf(_) => {}
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn is_leaf_patch(&self, node: &Node) -> bool {
        node.children
            .iter()
            .all(|child| self.tree_node(*child).is_leaf())
    }

    fn rejoin_leaf_patch_into(
        &mut self,
        parent_index: TreeIndex,
        level: usize,
        tree_index: TreeIndex,
        children: &[TreeIndex; 4],
    ) {
        self.rejoin_count += 1;

        // Free the other 3 node/leaf pairs.
        self.free_leaf(children[0]);
        self.free_leaf(children[1]);
        self.free_leaf(children[2]);
        self.free_leaf(children[3]);

        // Replace the current node patch as a leaf patch and free the prior leaf node.
        self.set_tree_node(
            tree_index,
            TreeNode::Leaf(Leaf {
                patch_index: self.tree_node(tree_index).patch_index(),
                parent: parent_index,
                level,
            }),
        );
    }

    fn subdivide_patch(&mut self, patch_index: PatchIndex, live_patches: &mut Vec<PatchIndex>) {
        println!("subdivide: {:?}", self.get_patch(patch_index).owner());
        assert!(self
            .tree_node(self.get_patch(patch_index).owner())
            .is_leaf());
        self.subdivide_count += 1;
        let current_level = self.tree_node(self.get_patch(patch_index).owner()).level();
        let next_level = current_level + 1;
        assert!(next_level <= MAX_LEVEL);
        let owner = self.get_patch(patch_index).owner();
        let [v0, v1, v2] = self.get_patch(patch_index).points().to_owned();
        let parent = self.tree_node(self.get_patch(patch_index).owner()).parent();

        // Get new points.
        let a = Point3::from(
            IcoSphere::bisect_edge(&v0.coords, &v1.coords).normalize() * EARTH_RADIUS_KM,
        );
        let b = Point3::from(
            IcoSphere::bisect_edge(&v1.coords, &v2.coords).normalize() * EARTH_RADIUS_KM,
        );
        let c = Point3::from(
            IcoSphere::bisect_edge(&v2.coords, &v0.coords).normalize() * EARTH_RADIUS_KM,
        );

        // Allocate geometry to new patches.
        let children = [
            self.allocate_leaf(owner, next_level, [v0, a, c]),
            self.allocate_leaf(owner, next_level, [v1, b, a]),
            self.allocate_leaf(owner, next_level, [v2, c, b]),
            self.allocate_leaf(owner, next_level, [a, b, c]),
        ];

        for i in &children {
            live_patches.push(self.tree_node(*i).patch_index());
        }

        // Transform our leaf/patch into a node and clobber the old patch.
        self.set_tree_node(
            owner,
            TreeNode::Node(Node {
                children,
                parent,
                patch_index,
                level: current_level,
            }),
        );
    }

    pub fn num_patches(&self) -> usize {
        self.patches.len()
    }

    fn apply_distance_function(
        &mut self,
        eye_position: &Point3<f64>,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        self.apply_distance_function_inner(eye_position, 0, TreeIndex(0), live_patches);
    }

    fn apply_distance_function_inner(
        &mut self,
        eye_position: &Point3<f64>,
        level: usize,
        tree_index: TreeIndex,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        self.visit_count += 1;
        // TODO: select max level based on height?

        match self.tree_node(tree_index) {
            TreeNode::Root => {
                // We have already applied visibility at this level, so we just need to recurse.
                assert_eq!(level, 0);
                let children = self.root.children; // Clone to avoid dual-borrow.
                for maybe_index in &children {
                    if let Some(i) = maybe_index {
                        self.apply_distance_function_inner(
                            eye_position,
                            level + 1,
                            *i,
                            live_patches,
                        );
                    }
                }
            }
            TreeNode::Node(ref node) => {
                if !self
                    .get_patch(node.patch_index)
                    .keep(&self.cached_viewable_region, eye_position)
                {
                    return;
                }

                if node
                    .children
                    .iter()
                    .all(|i| self.leaf_is_outside_distance_function(eye_position, level + 1, *i))
                {
                    self.rejoin_leaf_patch_into(
                        node.parent,
                        node.level,
                        tree_index,
                        &node.children,
                    );

                    live_patches.push(self.tree_node(tree_index).patch_index());

                    return;
                }

                for i in &node.children {
                    self.apply_distance_function_inner(eye_position, level + 1, *i, live_patches);
                }
            }
            TreeNode::Leaf(ref leaf) => {
                /*
                println!(
                    "lvl: {}; len: {}",
                    level,
                    (self.patches[leaf.patch_index.0].point(1)
                        - self.patches[leaf.patch_index.0].point(0))
                    .magnitude()
                        * 1000f64
                        / 5f64
                );
                 */

                // Don't split leaves past max level.
                assert!(level <= MAX_LEVEL);

                if level < MAX_LEVEL
                    && self.leaf_is_inside_distance_function(eye_position, level, leaf.patch_index)
                {
                    self.subdivide_patch(leaf.patch_index, live_patches);
                    self.apply_distance_function_inner(
                        eye_position,
                        level,
                        tree_index,
                        live_patches,
                    );
                    return;
                }

                if self
                    .get_patch(leaf.patch_index)
                    .keep(&self.cached_viewable_region, eye_position)
                {
                    live_patches.push(leaf.patch_index)
                }
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn leaf_is_outside_distance_function(
        &self,
        eye_position: &Point3<f64>,
        level: usize,
        tree_index: TreeIndex,
    ) -> bool {
        assert!(level > 0);
        let node = self.tree_node(tree_index);
        if !node.is_leaf() {
            return false;
        }
        let patch = self.get_patch(node.patch_index());
        let d2 = patch.distance_squared_to(eye_position);
        d2 > self.depth_levels[level - 1]
    }

    fn leaf_is_inside_distance_function(
        &self,
        eye_position: &Point3<f64>,
        level: usize,
        patch_index: PatchIndex,
    ) -> bool {
        assert!(level > 0);
        let patch = self.get_patch(patch_index);
        let d2 = patch.distance_squared_to(eye_position);
        d2 < self.depth_levels[level]
    }

    fn format_tree_display(&self) -> String {
        self.format_tree_display_inner(0, self.tree_root())
    }

    fn format_tree_display_inner(&self, lvl: usize, node: TreeNode) -> String {
        let mut out = String::new();
        match node {
            TreeNode::Root => {
                out += "Root\n";
                for maybe_child in &self.root.children {
                    if let Some(child) = maybe_child {
                        out += &self.format_tree_display_inner(lvl + 1, self.tree_node(*child));
                    }
                }
            }
            TreeNode::Node(ref node) => {
                let pad = "  ".repeat(lvl);
                out += &format!("{}Node: {:?}\n", pad, node.children);
                for child in &node.children {
                    out += &self.format_tree_display_inner(lvl + 1, self.tree_node(*child));
                }
            }
            TreeNode::Leaf(ref leaf) => {
                let pad = "  ".repeat(lvl);
                out += &format!("{}Leaf @{}, lvl: {}\n", pad, leaf.patch_index.0, lvl);
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
        return out;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_levels() {}
}
