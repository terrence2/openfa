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
use crate::{icosahedron::Icosahedron, patch::Patch};

use absolute_unit::Kilometers;
use camera::ArcBallCamera;
use geometry::{algorithm::bisect_edge, Plane};
use nalgebra::{Point3, Vector3};
use physical_constants::EARTH_RADIUS_KM;
use std::{cmp::Reverse, collections::BinaryHeap, time::Instant};

// Index into the tree vec. Note, debug builds do not handle the struct indirection well,
// so ironically release has better protections against misuse.

#[cfg(not(debug_assertions))]
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TreeIndex(usize);

#[cfg(not(debug_assertions))]
fn toff(ti: TreeIndex) -> usize {
    ti.0
}

#[cfg(debug_assertions)]
pub(crate) type TreeIndex = usize;

#[cfg(debug_assertions)]
#[allow(non_snake_case)]
fn TreeIndex(i: usize) -> usize {
    i
}

#[cfg(debug_assertions)]
fn toff(ti: TreeIndex) -> usize {
    ti
}

// Index into the patch vec.
#[cfg(not(debug_assertions))]
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct PatchIndex(pub(crate) usize);

#[cfg(not(debug_assertions))]
fn poff(pi: PatchIndex) -> usize {
    pi.0
}

#[cfg(debug_assertions)]
pub(crate) type PatchIndex = usize;

#[cfg(debug_assertions)]
#[allow(non_snake_case)]
fn PatchIndex(i: usize) -> usize {
    i
}

#[cfg(debug_assertions)]
fn poff(pi: TreeIndex) -> usize {
    pi
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TreeSibling {
    Peer(TreeIndex),
    Higher(TreeIndex),
    Lower(TreeIndex, TreeIndex),
    Uninitialized,
}

impl TreeSibling {
    fn is_higher(&self) -> bool {
        match self {
            Self::Higher(_) => true,
            _ => false,
        }
    }

    fn is_peer(&self) -> bool {
        match self {
            Self::Peer(_) => true,
            _ => false,
        }
    }

    fn is_lower(&self) -> bool {
        match self {
            Self::Lower(_, _) => true,
            _ => false,
        }
    }

    fn higher_node(&self) -> TreeIndex {
        match self {
            Self::Higher(ti) => *ti,
            _ => panic!("not a higher-level sibling"),
        }
    }

    fn peer_node(&self) -> TreeIndex {
        match self {
            Self::Peer(ti) => *ti,
            _ => panic!("not a peer-level sibling"),
        }
    }

    fn lower_nodes(&self) -> (TreeIndex, TreeIndex) {
        match self {
            Self::Lower(a, b) => (*a, *b),
            _ => panic!("not a lower-level sibling"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Root {
    children: [TreeIndex; 20],
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
    siblings: [TreeSibling; 3],
}

impl Leaf {
    fn find_peer_sibling(&self, target: TreeIndex) -> &TreeSibling {
        for sibling in self.siblings.iter() {
            if sibling.is_peer() && sibling.peer_node() == target {
                return sibling;
            }
        }
        panic!("no peer sibling matches")
    }

    fn find_peer_sibling_mut(&mut self, target: TreeIndex) -> &mut TreeSibling {
        for sibling in self.siblings.iter_mut() {
            if sibling.is_peer() && sibling.peer_node() == target {
                return sibling;
            }
        }
        panic!("no peer sibling matches")
    }

    fn find_higher_sibling(&self, target: TreeIndex) -> &TreeSibling {
        for sibling in self.siblings.iter() {
            if sibling.is_higher() && sibling.higher_node() == target {
                return sibling;
            }
        }
        panic!("no higher sibling matches")
    }

    fn find_higher_sibling_mut(&mut self, target: TreeIndex) -> &mut TreeSibling {
        for sibling in self.siblings.iter_mut() {
            if sibling.is_higher() && sibling.higher_node() == target {
                return sibling;
            }
        }
        panic!("no higher sibling matches")
    }

    fn find_lower_sibling(&self, target: TreeIndex) -> &TreeSibling {
        for sibling in self.siblings.iter() {
            if sibling.is_lower() {
                let (a, b) = sibling.lower_nodes();
                if a == target || b == target {
                    return sibling;
                }
            }
        }
        panic!("no lower sibling matches")
    }

    fn find_lower_sibling_mut(&mut self, target: TreeIndex) -> &mut TreeSibling {
        for sibling in self.siblings.iter_mut() {
            if sibling.is_lower() {
                let (a, b) = sibling.lower_nodes();
                if a == target || b == target {
                    return sibling;
                }
            }
        }
        panic!("no lower sibling matches")
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum TreeNode {
    Root,
    Node(Node),
    Leaf(Leaf),
    Empty,
}

impl TreeNode {
    fn is_leaf(&self) -> bool {
        match self {
            Self::Leaf(_) => true,
            _ => false,
        }
    }

    fn as_leaf(&self) -> &Leaf {
        match self {
            Self::Leaf(ref leaf) => leaf,
            _ => panic!("not a leaf node"),
        }
    }

    fn as_leaf_mut(&mut self) -> &mut Leaf {
        match self {
            Self::Leaf(ref mut leaf) => leaf,
            _ => panic!("not a leaf node"),
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

    // Panic if this is not a leaf or node.
    fn patch_index(&self) -> PatchIndex {
        match self {
            Self::Leaf(ref leaf) => leaf.patch_index,
            Self::Node(ref node) => node.patch_index,
            _ => panic!("Not a leaf!"),
        }
    }
}

pub(crate) struct PatchTree {
    max_level: usize,
    depth_levels: Vec<f64>,
    patches: Vec<Patch>,
    patch_empty_set: BinaryHeap<Reverse<PatchIndex>>,
    tree: Vec<TreeNode>,
    tree_empty_set: BinaryHeap<Reverse<TreeIndex>>,
    root: Root,

    subdivide_count: usize,
    rejoin_count: usize,
    visit_count: usize,

    cached_viewable_region: [Plane<f64>; 6],
    cached_eye_position: Point3<f64>,
    cached_eye_direction: Vector3<f64>,
}

impl PatchTree {
    pub(crate) fn new(max_level: usize, falloff_coefficient: f64) -> Self {
        let mut depth_levels = Vec::new();
        for i in 0..=max_level {
            let d = 1f64 * EARTH_RADIUS_KM * 2f64.powf(-(i as f64 * falloff_coefficient));
            depth_levels.push(d * d);
        }

        let sphere = Icosahedron::new();
        let cached_eye_position = Point3::new(0f64, 0f64, 0f64);
        let cached_eye_direction = Vector3::new(1f64, 0f64, 0f64);
        let cached_viewable_region =
            [Plane::from_normal_and_distance(Vector3::new(1f64, 0f64, 0f64), 0f64); 6];

        let mut patches = Vec::new();
        let mut tree = Vec::new();
        let mut root = Root {
            children: [TreeIndex(0); 20],
        };
        tree.push(TreeNode::Root);
        for (i, face) in sphere.faces.iter().enumerate() {
            let v0 = Point3::from(sphere.verts[face.i0()] * EARTH_RADIUS_KM);
            let v1 = Point3::from(sphere.verts[face.i1()] * EARTH_RADIUS_KM);
            let v2 = Point3::from(sphere.verts[face.i2()] * EARTH_RADIUS_KM);
            let mut p = Patch::new();
            p.change_target(TreeIndex(i + 1), [v0, v1, v2]);
            patches.push(p);
            tree.push(TreeNode::Leaf(Leaf {
                level: 1,
                parent: TreeIndex(0),
                patch_index: PatchIndex(i),
                siblings: [
                    TreeSibling::Peer(TreeIndex(face.siblings[0] + 1)),
                    TreeSibling::Peer(TreeIndex(face.siblings[1] + 1)),
                    TreeSibling::Peer(TreeIndex(face.siblings[2] + 1)),
                ],
            }));
            root.children[i] = TreeIndex(i + 1);
        }

        Self {
            max_level,
            depth_levels,
            patches,
            patch_empty_set: BinaryHeap::new(),
            tree,
            tree_empty_set: BinaryHeap::new(),
            root,
            subdivide_count: 0,
            rejoin_count: 0,
            visit_count: 0,
            cached_viewable_region,
            cached_eye_position,
            cached_eye_direction,
        }
    }

    pub(crate) fn get_patch(&self, index: PatchIndex) -> &Patch {
        &self.patches[poff(index)]
    }

    fn get_patch_mut(&mut self, index: PatchIndex) -> &mut Patch {
        &mut self.patches[poff(index)]
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
        patch_index
    }

    fn free_patch(&mut self, patch_index: PatchIndex) {
        self.get_patch_mut(patch_index).erect_tombstone();
        self.patch_empty_set.push(Reverse(patch_index));
    }

    fn allocate_tree_node(&mut self) -> TreeIndex {
        if let Some(Reverse(tree_index)) = self.tree_empty_set.pop() {
            return tree_index;
        }
        let tree_index = TreeIndex(self.tree.len());
        self.tree.push(TreeNode::Empty);
        tree_index
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
                siblings: [
                    TreeSibling::Uninitialized,
                    TreeSibling::Uninitialized,
                    TreeSibling::Uninitialized,
                ],
            }),
        );
        tree_index
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
        self.tree[toff(index)]
    }

    fn tree_node_mut(&mut self, index: TreeIndex) -> &mut TreeNode {
        &mut self.tree[toff(index)]
    }

    fn set_tree_node(&mut self, index: TreeIndex, node: TreeNode) {
        self.tree[toff(index)] = node;
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

        // Build a view-direction independent tesselation based on the current camera position.
        let reshape_start = Instant::now();
        self.apply_distance_function(live_patches);
        let reshape_time = Instant::now() - reshape_start;

        // Select patches based on visibility.
        println!(
            "patches: {} of {} [-{}] | nodes: {} [-{}] | -/+: {}/{}/{} | {:?}",
            live_patches.len(),
            self.patches.len(),
            self.patch_empty_set.len(),
            self.tree.len(),
            self.tree_empty_set.len(),
            self.rejoin_count,
            self.subdivide_count,
            self.visit_count,
            reshape_time,
        );
    }

    fn apply_distance_function(&mut self, live_patches: &mut Vec<PatchIndex>) {
        let children = self.root.children; // Clone to avoid dual-borrow.
        for i in &children {
            self.apply_distance_function_inner(1, *i, live_patches);
        }
    }

    fn apply_distance_function_inner(
        &mut self,
        level: usize,
        tree_index: TreeIndex,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        //println!("{}", self.format_tree_display());
        self.assert_subjective_integrity();
        self.visit_count += 1;

        match self.tree_node(tree_index) {
            TreeNode::Node(ref node) => {
                if !self
                    .get_patch(node.patch_index)
                    .keep(&self.cached_viewable_region)
                {
                    return;
                }

                if node
                    .children
                    .iter()
                    .all(|i| self.leaf_is_outside_distance_function(level + 1, *i))
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
                    self.apply_distance_function_inner(level + 1, *i, live_patches);
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
                assert!(level <= self.max_level);

                if level < self.max_level
                    && self.leaf_is_inside_distance_function(level, leaf.patch_index)
                {
                    self.subdivide_patch(leaf.patch_index, live_patches);
                    self.apply_distance_function_inner(level, tree_index, live_patches);
                    return;
                }

                if self
                    .get_patch(leaf.patch_index)
                    .keep(&self.cached_viewable_region)
                {
                    live_patches.push(leaf.patch_index)
                }
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
            TreeNode::Root => panic!("root node below top level of patch tree"),
        }
    }

    fn rejoin_leaf_patch_into(
        &mut self,
        parent_index: TreeIndex,
        level: usize,
        tree_index: TreeIndex,
        children: &[TreeIndex; 4],
    ) {
        self.rejoin_count += 1;

        // Handle siblings...
        let mut siblings = [TreeSibling::Uninitialized; 3];
        let siblings_0 = self.tree_node(children[0]).as_leaf().siblings;
        let siblings_1 = self.tree_node(children[1]).as_leaf().siblings;
        let siblings_2 = self.tree_node(children[2]).as_leaf().siblings;
        assert!(!siblings_0[0].is_lower());
        assert!(!siblings_0[2].is_lower());
        assert!(!siblings_1[0].is_lower());
        assert!(!siblings_1[1].is_lower());
        assert!(!siblings_2[1].is_lower());
        assert!(!siblings_2[2].is_lower());
        if siblings_0[0].is_peer() {
            assert!(siblings_1[0].is_peer());
            siblings[0] = TreeSibling::Lower(siblings_0[0].peer_node(), siblings_1[0].peer_node());
            {
                let other_0 = self
                    .tree_node_mut(siblings_0[0].peer_node())
                    .as_leaf_mut()
                    .find_peer_sibling_mut(children[0]);
                *other_0 = TreeSibling::Higher(tree_index);
            }
            {
                let other_1 = self
                    .tree_node_mut(siblings_1[0].peer_node())
                    .as_leaf_mut()
                    .find_peer_sibling_mut(children[1]);
                *other_1 = TreeSibling::Higher(tree_index);
            }
        } else if siblings_0[0].is_higher() {
            assert!(siblings_1[0].is_higher());
            assert_eq!(siblings_1[0].higher_node(), siblings_0[0].higher_node());
            siblings[0] = TreeSibling::Peer(siblings_0[0].higher_node());
            {
                println!("In node: {:?} w/ sib00: {:?}", tree_index, siblings_0[0]);
                let other = self
                    .tree_node_mut(siblings_0[0].higher_node())
                    .as_leaf_mut()
                    .find_lower_sibling_mut(children[0]);
                *other = TreeSibling::Peer(tree_index);
            }
        }
        if siblings_1[1].is_peer() {
            assert!(siblings_2[1].is_peer());
            siblings[1] = TreeSibling::Lower(siblings_1[1].peer_node(), siblings_2[1].peer_node());
            {
                let other_1 = self
                    .tree_node_mut(siblings_1[1].peer_node())
                    .as_leaf_mut()
                    .find_peer_sibling_mut(children[1]);
                *other_1 = TreeSibling::Higher(tree_index);
            }
            {
                let other_2 = self
                    .tree_node_mut(siblings_2[1].peer_node())
                    .as_leaf_mut()
                    .find_peer_sibling_mut(children[2]);
                *other_2 = TreeSibling::Higher(tree_index);
            }
        } else if siblings_1[1].is_higher() {
            assert!(siblings_2[1].is_higher());
            assert_eq!(siblings_2[1].higher_node(), siblings_1[1].higher_node());
            siblings[1] = TreeSibling::Peer(siblings_1[1].higher_node());
            {
                let other = self
                    .tree_node_mut(siblings_1[1].higher_node())
                    .as_leaf_mut()
                    .find_lower_sibling_mut(children[1]);
                *other = TreeSibling::Peer(tree_index);
            }
        }
        if siblings_2[2].is_peer() {
            assert!(siblings_0[2].is_peer());
            siblings[2] = TreeSibling::Lower(siblings_2[2].peer_node(), siblings_0[2].peer_node());
            {
                let other_2 = self
                    .tree_node_mut(siblings_2[2].peer_node())
                    .as_leaf_mut()
                    .find_peer_sibling_mut(children[2]);
                *other_2 = TreeSibling::Higher(tree_index);
            }
            {
                let other_0 = self
                    .tree_node_mut(siblings_0[2].peer_node())
                    .as_leaf_mut()
                    .find_peer_sibling_mut(children[0]);
                *other_0 = TreeSibling::Higher(tree_index);
            }
        } else if siblings_2[2].is_higher() {
            assert!(siblings_0[2].is_higher());
            assert_eq!(siblings_0[2].higher_node(), siblings_2[2].higher_node());
            siblings[2] = TreeSibling::Peer(siblings_2[2].higher_node());
            {
                let other = self
                    .tree_node_mut(siblings_2[2].higher_node())
                    .as_leaf_mut()
                    .find_lower_sibling_mut(children[2]);
                *other = TreeSibling::Peer(tree_index);
            }
        }

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
                siblings,
            }),
        );
    }

    fn subdivide_patch(&mut self, patch_index: PatchIndex, live_patches: &mut Vec<PatchIndex>) {
        let owner = self.get_patch(patch_index).owner();
        let parent = self.tree_node(owner).parent();
        assert!(self.tree_node(owner).is_leaf());

        let node = self.tree_node(owner);
        for sibling in &node.as_leaf().siblings {
            // do recursive split
            if sibling.is_higher() {
                self.subdivide_patch(
                    self.tree_node(sibling.higher_node()).patch_index(),
                    live_patches,
                );
            }
        }
        let node = self.tree_node(owner);
        for sibling in &node.as_leaf().siblings {
            assert!(!sibling.is_higher());
        }

        println!("subdivide: {:?}", self.get_patch(patch_index).owner());
        self.subdivide_count += 1;
        let current_level = self.tree_node(self.get_patch(patch_index).owner()).level();
        let next_level = current_level + 1;
        assert!(next_level <= self.max_level);
        let [v0, v1, v2] = self.get_patch(patch_index).points().to_owned();

        // Get new points.
        let a = Point3::from(bisect_edge(&v0.coords, &v1.coords).normalize() * EARTH_RADIUS_KM);
        let b = Point3::from(bisect_edge(&v1.coords, &v2.coords).normalize() * EARTH_RADIUS_KM);
        let c = Point3::from(bisect_edge(&v2.coords, &v0.coords).normalize() * EARTH_RADIUS_KM);

        // Allocate geometry to new patches.
        let children = [
            self.allocate_leaf(owner, next_level, [v0, a, c]),
            self.allocate_leaf(owner, next_level, [v1, b, a]),
            self.allocate_leaf(owner, next_level, [v2, c, b]),
            self.allocate_leaf(owner, next_level, [c, a, b]),
        ];

        // Fill in internal sibling edges.
        {
            let child_0 = self.tree_node_mut(children[0]).as_leaf_mut();
            child_0.siblings[1] = TreeSibling::Peer(children[3]);
        }
        {
            let child_1 = self.tree_node_mut(children[1]).as_leaf_mut();
            child_1.siblings[2] = TreeSibling::Peer(children[3]);
        }
        {
            let child_2 = self.tree_node_mut(children[2]).as_leaf_mut();
            child_2.siblings[0] = TreeSibling::Peer(children[3]);
        }
        {
            let child_3 = self.tree_node_mut(children[3]).as_leaf_mut();
            child_3.siblings[0] = TreeSibling::Peer(children[0]);
            child_3.siblings[1] = TreeSibling::Peer(children[1]);
            child_3.siblings[2] = TreeSibling::Peer(children[2]);
        }

        // Fill in external edges
        let siblings = self.tree_node(owner).as_leaf().siblings;
        // For edge01
        if siblings[0].is_peer() {
            // peer -> higher/lower
            {
                let child_0 = self.tree_node_mut(children[0]).as_leaf_mut();
                child_0.siblings[0] = TreeSibling::Higher(siblings[0].peer_node());
            }
            {
                let child_1 = self.tree_node_mut(children[1]).as_leaf_mut();
                child_1.siblings[0] = TreeSibling::Higher(siblings[0].peer_node());
            }
            println!(
                "PEER: {:?} => {:?}",
                siblings[0].peer_node(),
                self.tree_node(siblings[0].peer_node())
            );
            let other = self
                .tree_node_mut(siblings[0].peer_node())
                .as_leaf_mut()
                .find_peer_sibling_mut(owner);
            *other = TreeSibling::Lower(children[0], children[1]);
        } else if siblings[0].is_lower() {
            let (a, b) = siblings[0].lower_nodes();
            // higher/lower -> peer
            {
                let child_0 = self.tree_node_mut(children[0]).as_leaf_mut();
                child_0.siblings[0] = TreeSibling::Peer(a);
            }
            {
                let child_1 = self.tree_node_mut(children[1]).as_leaf_mut();
                child_1.siblings[0] = TreeSibling::Peer(b);
            }
            {
                let other_a = self
                    .tree_node_mut(a)
                    .as_leaf_mut()
                    .find_higher_sibling_mut(owner);
                *other_a = TreeSibling::Peer(children[0]);
            }
            let other_b = self
                .tree_node_mut(b)
                .as_leaf_mut()
                .find_higher_sibling_mut(owner);
            *other_b = TreeSibling::Peer(children[1]);
        }

        // For edge12
        if siblings[1].is_peer() {
            // peer -> higher/lower
            {
                let child_1 = self.tree_node_mut(children[1]).as_leaf_mut();
                child_1.siblings[1] = TreeSibling::Higher(siblings[1].peer_node());
            }
            {
                let child_2 = self.tree_node_mut(children[2]).as_leaf_mut();
                child_2.siblings[1] = TreeSibling::Higher(siblings[1].peer_node());
            }
            println!(
                "PEER: {:?} => {:?}",
                siblings[1].peer_node(),
                self.tree_node(siblings[1].peer_node())
            );
            let other = self
                .tree_node_mut(siblings[1].peer_node())
                .as_leaf_mut()
                .find_peer_sibling_mut(owner);
            *other = TreeSibling::Lower(children[1], children[2]);
        } else if siblings[1].is_lower() {
            let (a, b) = siblings[1].lower_nodes();
            // higher/lower -> peer
            {
                let child_1 = self.tree_node_mut(children[1]).as_leaf_mut();
                child_1.siblings[1] = TreeSibling::Peer(a);
            }
            {
                let child_2 = self.tree_node_mut(children[2]).as_leaf_mut();
                child_2.siblings[1] = TreeSibling::Peer(b);
            }
            {
                let other_a = self
                    .tree_node_mut(a)
                    .as_leaf_mut()
                    .find_higher_sibling_mut(owner);
                *other_a = TreeSibling::Peer(children[1]);
            }
            let other_b = self
                .tree_node_mut(b)
                .as_leaf_mut()
                .find_higher_sibling_mut(owner);
            *other_b = TreeSibling::Peer(children[2]);
        }

        // For edge20
        if siblings[2].is_peer() {
            // peer -> higher/lower
            {
                let child_2 = self.tree_node_mut(children[2]).as_leaf_mut();
                child_2.siblings[2] = TreeSibling::Higher(siblings[2].peer_node());
            }
            {
                let child_0 = self.tree_node_mut(children[0]).as_leaf_mut();
                child_0.siblings[2] = TreeSibling::Higher(siblings[2].peer_node());
            }
            println!(
                "PEER: {:?} => {:?}",
                siblings[2].peer_node(),
                self.tree_node(siblings[2].peer_node())
            );
            let other = self
                .tree_node_mut(siblings[2].peer_node())
                .as_leaf_mut()
                .find_peer_sibling_mut(owner);
            *other = TreeSibling::Lower(children[2], children[0]);
        } else if siblings[2].is_lower() {
            let (a, b) = siblings[2].lower_nodes();
            // higher/lower -> peer
            {
                let child_2 = self.tree_node_mut(children[2]).as_leaf_mut();
                child_2.siblings[2] = TreeSibling::Peer(a);
            }
            {
                let child_0 = self.tree_node_mut(children[0]).as_leaf_mut();
                child_0.siblings[2] = TreeSibling::Peer(b);
            }
            {
                let other_a = self
                    .tree_node_mut(a)
                    .as_leaf_mut()
                    .find_higher_sibling_mut(owner);
                *other_a = TreeSibling::Peer(children[2]);
            }
            let other_b = self
                .tree_node_mut(b)
                .as_leaf_mut()
                .find_higher_sibling_mut(owner);
            *other_b = TreeSibling::Peer(children[0]);
        }

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

    fn leaf_is_outside_distance_function(&self, level: usize, tree_index: TreeIndex) -> bool {
        assert!(level > 0);
        let node = self.tree_node(tree_index);
        if !node.is_leaf() {
            return false;
        }
        for sibling in &node.as_leaf().siblings {
            if sibling.is_lower() {
                return false;
            }
        }
        let patch = self.get_patch(node.patch_index());
        let d2 = patch.distance_squared_to(&self.cached_eye_position);
        d2 > self.depth_levels[level - 1]
    }

    fn leaf_is_inside_distance_function(&self, level: usize, patch_index: PatchIndex) -> bool {
        assert!(level > 0);
        let patch = self.get_patch(patch_index);
        let d2 = patch.distance_squared_to(&self.cached_eye_position);
        d2 < self.depth_levels[level]
    }

    fn assert_subjective_integrity(&self) {
        let mut path = Vec::new();
        self.assert_subjective_integrity_inner(0, TreeIndex(0), &mut path);
    }

    fn assert_subjective_integrity_inner(
        &self,
        level: usize,
        tree_index: TreeIndex,
        path: &mut Vec<TreeIndex>,
    ) {
        path.push(tree_index);
        match self.tree_node(tree_index) {
            TreeNode::Root => {
                assert_eq!(level, 0);
                for child in &self.root.children {
                    self.assert_subjective_integrity_inner(level + 1, *child, path);
                }
            }
            TreeNode::Node(ref node) => {
                assert_eq!(node.level, level);
                assert_eq!(self.get_patch(node.patch_index).owner(), tree_index);
                for child in &node.children {
                    self.assert_subjective_integrity_inner(level + 1, *child, path);
                }
            }
            TreeNode::Leaf(ref leaf) => {
                assert_eq!(leaf.level, level);
                assert_eq!(self.get_patch(leaf.patch_index).owner(), tree_index);
                for &sibling in &leaf.siblings {
                    match sibling {
                        TreeSibling::Peer(peer_index) => {
                            assert_eq!(self.tree_node(peer_index).level(), level);
                            assert!(self.tree_node(peer_index).is_leaf());
                            assert_eq!(
                                self.tree_node(peer_index)
                                    .as_leaf()
                                    .find_peer_sibling(tree_index)
                                    .peer_node(),
                                tree_index
                            );
                        }
                        TreeSibling::Higher(higher_index) => {
                            assert_eq!(self.tree_node(higher_index).level(), level - 1);
                            assert!(self.tree_node(higher_index).is_leaf());
                            let (a, b) = self
                                .tree_node(higher_index)
                                .as_leaf()
                                .find_lower_sibling(tree_index)
                                .lower_nodes();
                            assert!(tree_index == a || tree_index == b);
                        }
                        TreeSibling::Lower(lower_a, lower_b) => {
                            assert_eq!(self.tree_node(lower_a).level(), level + 1);
                            assert_eq!(self.tree_node(lower_b).level(), level + 1);
                            assert!(self.tree_node(lower_a).is_leaf());
                            assert!(self.tree_node(lower_b).is_leaf());
                        }
                        TreeSibling::Uninitialized => panic!("uninitialized sibling at {:?}", path),
                    }
                }
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
        path.pop();
    }

    #[allow(unused)]
    fn format_tree_display(&self) -> String {
        self.format_tree_display_inner(0, TreeIndex(0))
    }

    #[allow(unused)]
    fn format_tree_display_inner(&self, lvl: usize, tree_index: TreeIndex) -> String {
        let mut out = String::new();
        let node = self.tree_node(tree_index);
        match node {
            TreeNode::Root => {
                out += "Root @0\n";
                for child in &self.root.children {
                    out += &self.format_tree_display_inner(lvl + 1, *child);
                }
            }
            TreeNode::Node(ref node) => {
                let pad = "  ".repeat(lvl);
                out += &format!("{}Node @{:?}: {:?}\n", pad, tree_index, node.children);
                for child in &node.children {
                    out += &self.format_tree_display_inner(lvl + 1, *child);
                }
            }
            TreeNode::Leaf(ref leaf) => {
                let pad = "  ".repeat(lvl);
                out += &format!(
                    "{}Leaf @{:?}, lvl: {}, p: {}, sib: {:?}\n",
                    pad,
                    tree_index,
                    lvl,
                    poff(leaf.patch_index),
                    leaf.siblings
                );
            }
            TreeNode::Empty => panic!("empty node in patch tree"),
        }
        out
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use absolute_unit::meters;

    #[test]
    fn test_basic() {
        let mut tree = PatchTree::new(15, 0.8);
        let mut live_patches = Vec::new();
        let camera = ArcBallCamera::new(16.0 / 9.0, meters!(0.1), meters!(10_000));
        tree.optimize_for_view(&camera, &mut live_patches);
    }
}
