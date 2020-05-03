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
use geometry::{IcoSphere, Plane};
use nalgebra::{Point3, Vector3};
use std::{cmp::Ordering, collections::HashSet, time::Instant};
use universe::EARTH_RADIUS_KM;

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
            Self::Leaf(leaf) => return leaf.patch_index,
            _ => panic!("Not a leaf!"),
        }
    }
}

pub(crate) struct PatchTree {
    num_patches: usize,
    sphere: IcoSphere,
    depth_levels: Vec<f64>,
    patches: Vec<Patch>,
    patch_empty_set: Vec<PatchIndex>,
    tree: Vec<TreeNode>,
    tree_empty_set: Vec<TreeIndex>,
    root: Root,
    root_patches: [Patch; 20],

    subdivide_count: usize,
    rejoin_count: usize,

    cached_viewable_region: [Plane<f64>; 6],
    cached_eye_position: Point3<f64>,
    cached_eye_direction: Vector3<f64>,
}

impl PatchTree {
    pub(crate) fn new(num_patches: usize) -> Self {
        const LEVEL_COUNT: usize = 40;
        let mut depth_levels = Vec::new();
        for i in 0..LEVEL_COUNT {
            let d = 2f64 * EARTH_RADIUS_KM * 2f64.powf(-(i as f64));
            depth_levels.push(d * d);
        }

        let sphere = IcoSphere::new(0);
        let mut patches = Vec::with_capacity(num_patches);
        let mut tree = Vec::new();
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
            patch_empty_set: Vec::new(),
            tree,
            tree_empty_set: Vec::new(),
            root,
            root_patches,
            subdivide_count: 0,
            rejoin_count: 0,
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

    fn allocate_patch(&mut self) -> PatchIndex {
        if let Some(patch_index) = self.patch_empty_set.pop() {
            return patch_index;
        }
        let patch_index = PatchIndex(self.patches.len());
        self.patches.push(Patch::new());
        return patch_index;
    }

    fn free_patch(&mut self, patch_index: PatchIndex) {
        self.get_patch_mut(patch_index).erect_tombstone();
        self.patch_empty_set.push(patch_index);
    }

    fn patch_count(&self) -> usize {
        self.patches.len() - self.patch_empty_set.len()
    }

    fn allocate_tree_node(&mut self) -> TreeIndex {
        if let Some(tree_index) = self.tree_empty_set.pop() {
            return tree_index;
        }
        let tree_index = TreeIndex(self.tree.len());
        self.tree.push(TreeNode::Empty);
        return tree_index;
    }

    fn free_tree_node(&mut self, tree_index: TreeIndex) {
        self.set_tree_node(tree_index, TreeNode::Empty);
        self.tree_empty_set.push(tree_index);
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
        /*
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
         */
    }

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
        debug_verts: &mut Vec<DebugVertex>,
    ) {
        self.subdivide_count = 0;
        self.rejoin_count = 0;

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
        self.apply_distance_function(&eye_position);

        // Select patches based on visibility.
        println!(
            "patches [free]: {} [{}] | nodes [free]: {} [{}] | -/+: {}/{}\n{}\nEMPTY: {:?}\nNODES:",
            self.patches.len(),
            self.patch_empty_set.len(),
            self.tree.len(),
            self.tree_empty_set.len(),
            self.rejoin_count,
            self.subdivide_count,
            self.format_tree_display(),
            self.tree_empty_set,
        );
        for node in &self.tree {
            println!("  {:?}", node);
        }

        /*
        let mut hidden = 0;
        for p in &self.patches {
            if !p.keep(&self.cached_viewable_region, &self.cached_eye_position) {
                hidden += 1;
            }
        }

        println!("hidden: {} | {}", hidden, hidden_rejoins);
         */
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

        // We enter with 5 nodes and 4 patches.
        // We want to leave with 1 node and 1 patch.

        // Note: we do not re-use any of the vertices from the inner patch.
        let i0 = self.tree_node(children[0]).patch_index();
        let i1 = self.tree_node(children[1]).patch_index();
        let i2 = self.tree_node(children[2]).patch_index();
        let v0 = *self.get_patch(i0).point(0);
        let v1 = *self.get_patch(i1).point(0);
        let v2 = *self.get_patch(i2).point(0);

        // Re-use the patch at i0 so we don't have to flex to allocate a new one.
        self.get_patch_mut(i0)
            .change_target(tree_index, [v0, v1, v2]);

        // Free the other 3 node/leaf pairs.
        self.free_leaf(children[1]);
        self.free_leaf(children[2]);
        self.free_leaf(children[3]);

        // Replace the current node patch as a leaf patch and free the prior leaf node.
        self.free_tree_node(children[0]);
        self.set_tree_node(
            tree_index,
            TreeNode::Leaf(Leaf {
                patch_index: i0,
                parent: parent_index,
                level,
            }),
        );
    }

    fn subdivide_patch(&mut self, patch_index: PatchIndex) {
        self.subdivide_count += 1;
        let current_level = self.tree_node(self.get_patch(patch_index).owner()).level();
        let level = current_level + 1;
        assert!(level < 15);
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
            self.allocate_leaf(owner, level, [v0, a, c]),
            self.allocate_leaf(owner, level, [v1, b, a]),
            self.allocate_leaf(owner, level, [v2, c, b]),
            self.allocate_leaf(owner, level, [a, b, c]),
        ];

        // Transform our leaf/patch into a node and clobber the old patch.
        self.set_tree_node(
            owner,
            TreeNode::Node(Node {
                children,
                parent,
                level: current_level,
            }),
        );
        self.free_patch(patch_index);
    }

    pub fn num_patches(&self) -> usize {
        self.patches.len()
    }

    fn apply_distance_function(&mut self, eye_position: &Point3<f64>) {
        self.apply_distance_function_inner(eye_position, 0, TreeIndex(0));
    }

    fn apply_distance_function_inner(
        &mut self,
        eye_position: &Point3<f64>,
        level: usize,
        tree_index: TreeIndex,
    ) {
        // Note: select max level based on height?
        const MAX_LEVEL: usize = 4;

        match self.tree_node(tree_index) {
            TreeNode::Root => {
                // We have already applied visibility at this level, so we just need to recurse.
                assert_eq!(level, 0);
                let children = self.root.children; // Clone to avoid dual-borrow.
                for maybe_index in &children {
                    if let Some(i) = maybe_index {
                        self.apply_distance_function_inner(eye_position, level + 1, *i);
                    }
                }
            }
            TreeNode::Node(ref node) => {
                // Recurse first so that patches that get rejoined are available to re-allocate later.
                for i in &node.children {
                    self.apply_distance_function_inner(eye_position, level + 1, *i);
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
                }
            }
            TreeNode::Leaf(ref leaf) => {
                // Don't split leaves past max level.
                if level >= MAX_LEVEL {
                    return;
                }

                if self.leaf_is_inside_distance_function(eye_position, level, leaf.patch_index) {
                    self.subdivide_patch(leaf.patch_index)
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

    /*
    fn rejoin_tree_to_depth(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        node_index: TreeIndex,
    ) {
        match self.tree_node(node_index) {
            TreeNode::Root => {
                /*
                for i in 0..20 {
                    if children[i].is_none() && self.root_patches[i].keep(camera, horizon_plane, eye_position) {
                    }
                }
                */
                let children = self.root.children;
                for i in &children {
                    self.rejoin_tree_to_depth(camera, horizon_plane, eye_position, *i);
                }
            }
            TreeNode::Node(ref node) => {
                for i in &node.children {
                    self.rejoin_tree_to_depth(camera, horizon_plane, eye_position, *i);
                }
                if node.children.iter().all(|child| {
                    self.leaf_can_be_rejoined(
                        camera,
                        horizon_plane,
                        eye_position,
                        self.tree_node(*child),
                    )
                }) {
                    let new_child = self.rejoin_patch(&node.children);
                    self.set_tree_node(node_index, new_child);
                }
            }
            TreeNode::Leaf(_) => {}
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
        TreeNode::Leaf(Leaf { offset: i0 })
    }

    fn leaf_can_be_rejoined(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        node: TreeNode,
    ) -> bool {
        match node {
            TreeNode::Root => false,
            TreeNode::Node(_) => false,
            TreeNode::Leaf(leaf) => {
                let patch = self.get_patch(leaf.offset);
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
            TreeNode::Root => {
                let children = self.root.children;
                for i in &children {
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
                for i in &children {
                    self.subdivide_tree_to_depth(
                        camera,
                        horizon_plane,
                        eye_position,
                        eye_direction,
                        self.tree_node(*i),
                    );
                }
            }
            TreeNode::Node(ref node) => {
                for i in &node.children {
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
                for i in &node.children {
                    self.subdivide_tree_to_depth(
                        camera,
                        horizon_plane,
                        eye_position,
                        &eye_direction,
                        self.tree_node(*i),
                    );
                }
            }
            TreeNode::Leaf(_) => {}
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
            TreeNode::Root => None,
            TreeNode::Node(_) => None,
            TreeNode::Leaf(leaf) => {
                let (maybe_offsets, patch_pts, patch_level) = {
                    let patch = self.get_patch(leaf.offset);
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
                self.get_patch_mut(leaf.offset).erect_tombstone();

                let [pt0off, pt1off, pt2off, pt3off] = self.find_empty_tree_slots();
                self.set_tree_node(pt0off, TreeNode::Leaf(Leaf { offset: p0off }));
                self.set_tree_node(pt1off, TreeNode::Leaf(Leaf { offset: p1off }));
                self.set_tree_node(pt2off, TreeNode::Leaf(Leaf { offset: p2off }));
                self.set_tree_node(pt3off, TreeNode::Leaf(Leaf { offset: p3off }));

                return Some(TreeNode::Node(Node {
                    children: [pt0off, pt1off, pt2off, pt3off],
                }));
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }
    */
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

    /*
     fn previous_optimize() {
        let rejoin_start = Instant::now();
        let max_levels = 10;
        for lvl in 1..=max_levels {
            self.visit_at_level(
                &camera,
                &horizon_plane,
                &eye_position,
                0,
                max_levels - lvl,
                TreeIndex(0),
                Goal::Rejoin,
            );
        }
        let rejoin_time = Instant::now() - rejoin_start;

        let divide_start = Instant::now();
        for lvl in 0..max_levels {
            self.visit_at_level(
                &camera,
                &horizon_plane,
                &eye_position,
                0,
                lvl,
                TreeIndex(0),
                Goal::Subdivide,
            );
        }
        let divide_time = Instant::now() - divide_start;

        let max_levels = 10;
        for lvl in 0..max_levels {
            let mut nodes_at_level = Vec::with_capacity(self.num_patches * 2);
            self.collect_at_level(
                &camera,
                &horizon_plane,
                &eye_position,
                0,
                lvl,
                TreeIndex(0),
                &mut nodes_at_level,
            );
        }

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

        // Split patches until we have an optimal equal-area partitioning.
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
    */

    /*
    fn visit_at_level(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        current_level: usize,
        target_level: usize,
        node_index: TreeIndex,
        goal: Goal,
    ) {
        if current_level == target_level {
            self.count_visit += 1;
            match goal {
                Goal::Subdivide => self.visit_subdivide_at_level(
                    camera,
                    horizon_plane,
                    eye_position,
                    current_level,
                    target_level,
                    node_index,
                ),
                Goal::Rejoin => self.visit_rejoin_at_level(
                    camera,
                    horizon_plane,
                    eye_position,
                    current_level,
                    target_level,
                    node_index,
                ),
            }
            return;
        }

        // Recurse to proper level. Note that we visit from top down, so
        // we will have split to depth before reaching this point.
        assert!(current_level < target_level);
        match self.tree_node(node_index) {
            TreeNode::Root => {
                let children = self.root.children;
                for maybe_child in &children {
                    if let Some(child) = maybe_child {
                        self.visit_at_level(
                            camera,
                            horizon_plane,
                            eye_position,
                            current_level + 1,
                            target_level,
                            *child,
                            goal,
                        );
                    }
                }
            }
            TreeNode::Node(ref node) => {
                for i in &node.children {
                    //assert!(self.tree_node(*i).parent);
                    self.visit_at_level(
                        camera,
                        horizon_plane,
                        eye_position,
                        current_level + 1,
                        target_level,
                        *i,
                        goal,
                    );
                }
            }
            TreeNode::Leaf(_) => {}
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn visit_subdivide_at_level(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        current_level: usize,
        target_level: usize,
        node_index: TreeIndex,
    ) {
        assert_eq!(current_level, target_level);
        match self.tree_node(node_index) {
            TreeNode::Root => {
                for i in 0..20 {
                    if self.root_patches[i].keep(camera, horizon_plane, eye_position) {
                        if self.root.children[i].is_none() {
                            // Add newly visible root patch to patch tree.
                            // FIXME: reclaim by rejoining.
                            let patch_index = self.find_empty_patch_slot().unwrap();
                            let tree_index = self.find_or_create_empty_tree_slot();
                            *self.get_patch_mut(patch_index) = self.root_patches[i];
                            self.set_tree_node(
                                tree_index,
                                TreeNode::Leaf(Leaf {
                                    parent: TreeIndex(0),
                                    patch_index: patch_index,
                                }),
                            );
                            self.root.children[i] = Some(tree_index);
                        }
                    } else {
                        if let Some(tree_index) = self.root.children[i] {
                            // Remove newly invisible root patch from patch tree.
                            self.collapse_node_to_leaf(tree_index);
                            assert!(self.tree_node(tree_index).is_leaf(), "trying to remove root patch that is not a leaf! How did we get over the horizon while still being close enough to be subdivided?");
                            let patch_index = self.tree_node(tree_index).leaf_patch();
                            self.get_patch_mut(patch_index).erect_tombstone();
                            self.set_tree_node(tree_index, TreeNode::Empty);
                            self.root.children[i] = None;
                        }
                    }
                }
            }
            TreeNode::Node(_) => {}
            TreeNode::Leaf(ref leaf) => {
                // First constraint: subdivide at least as far as level.
                if self.leaf_can_be_subdivided(eye_position, leaf) {
                    if let Some(new_node) = self.subdivide_patch_1(node_index, leaf) {
                        self.set_tree_node(node_index, new_node);
                    } else {
                        //panic!("would de-divide");
                    }
                }
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn leaf_can_be_subdivided(&self, eye_position: &Point3<f64>, leaf: &Leaf) -> bool {
        let patch = self.get_patch(leaf.patch_index);
        let d2 = patch.distance_squared_to(eye_position);
        return d2 <= self.depth_levels[patch.level()];
    }

    fn leaf_can_be_rejoined(
        &self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        node: TreeNode,
    ) -> bool {
        match node {
            TreeNode::Root => false,
            TreeNode::Node(_) => false,
            TreeNode::Leaf(ref leaf) => {
                let patch = self.get_patch(leaf.patch_index);
                let d2 = patch.distance_squared_to(eye_position);
                assert!(patch.level() > 0);
                d2 > self.depth_levels[patch.level().max(2) - 2]
                    || !patch.keep(camera, horizon_plane, eye_position)
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }

    fn visit_rejoin_at_level(
        &mut self,
        camera: &ArcBallCamera,
        horizon_plane: &Plane<f64>,
        eye_position: &Point3<f64>,
        current_level: usize,
        target_level: usize,
        node_index: TreeIndex,
    ) {
        assert_eq!(current_level, target_level);
        match self.tree_node(node_index) {
            TreeNode::Root => {}
            TreeNode::Node(node) => {
                if node.children.iter().all(|child| {
                    self.leaf_can_be_rejoined(
                        camera,
                        horizon_plane,
                        eye_position,
                        self.tree_node(*child),
                    )
                }) {
                    self.rejoin_leaf_patch_into(node.parent, node_index, &node.children);
                }
            }
            TreeNode::Leaf(_) => {}
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
    }
    */
    /*
    fn subdivide_patch_1(&mut self, leaf_index: TreeIndex, leaf: &Leaf) -> Option<TreeNode> {
        self.count_subdivide += 1;

        let patch_index = leaf.patch_index;
        let patch_level = self.get_patch(patch_index).level();

        let maybe_offsets = self.find_empty_patch_slots();
        if maybe_offsets.is_none() {
            if let Some(tree_index) = self.find_node_to_collapse(patch_level) {
                println!("collapsing patch");
                if let TreeNode::Node(ref node) = self.tree_node(tree_index) {
                    self.rejoin_leaf_patch_into(node.parent, tree_index, &node.children);
                    return self.subdivide_patch_1(leaf_index, leaf);
                }
            }
            println!("no patches to collapse");
            return None;
        }
        let offsets = maybe_offsets.unwrap();

        let [v0, v1, v2] = self.get_patch(patch_index).points().to_owned();
        let a = Point3::from(
            IcoSphere::bisect_edge(&v0.coords, &v1.coords).normalize() * EARTH_RADIUS_KM,
        );
        let b = Point3::from(
            IcoSphere::bisect_edge(&v1.coords, &v2.coords).normalize() * EARTH_RADIUS_KM,
        );
        let c = Point3::from(
            IcoSphere::bisect_edge(&v2.coords, &v0.coords).normalize() * EARTH_RADIUS_KM,
        );
        let [p0off, p1off, p2off, p3off] = offsets;
        let [pt0off, pt1off, pt2off, pt3off] = self.find_empty_tree_slots();
        self.get_patch_mut(p0off)
            .change_target(pt0off, patch_level + 1, [v0, a, c]);
        self.set_tree_node(
            pt0off,
            TreeNode::Leaf(Leaf {
                parent: leaf_index,
                patch_index: p0off,
            }),
        );

        self.get_patch_mut(p1off)
            .change_target(pt1off, patch_level + 1, [v1, b, a]);
        self.set_tree_node(
            pt1off,
            TreeNode::Leaf(Leaf {
                parent: leaf_index,
                patch_index: p1off,
            }),
        );

        self.get_patch_mut(p2off)
            .change_target(pt2off, patch_level + 1, [v2, c, b]);
        self.set_tree_node(
            pt2off,
            TreeNode::Leaf(Leaf {
                parent: leaf_index,
                patch_index: p2off,
            }),
        );

        self.get_patch_mut(p3off)
            .change_target(pt3off, patch_level + 1, [a, b, c]);
        self.set_tree_node(
            pt3off,
            TreeNode::Leaf(Leaf {
                parent: leaf_index,
                patch_index: p3off,
            }),
        );

        self.get_patch_mut(leaf.patch_index).erect_tombstone();

        return Some(TreeNode::Node(Node {
            parent: leaf.parent,
            children: [pt0off, pt1off, pt2off, pt3off],
        }));
    }

    fn find_node_to_collapse(&self, lower_than_level: usize) -> Option<TreeIndex> {
        for (i, patch) in self.patches.iter().enumerate() {
            if patch.is_alive() && patch.level() > lower_than_level {
                let tree_leaf_index = patch.owner();
                let leaf_node = self.tree_node(tree_leaf_index);
                assert!(leaf_node.is_leaf());
                let parent_node = self.tree_node(leaf_node.parent());
                if parent_node.is_node() && self.is_leaf_patch(parent_node.as_node()) {
                    return Some(leaf_node.parent());
                } else {
                    println!("  not a leaf patch");
                }
            } else {
                println!("  not less than level");
            }
        }
        None
    }
    */
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_levels() {}
}
