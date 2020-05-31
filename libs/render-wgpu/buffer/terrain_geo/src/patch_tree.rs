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
use approx::assert_relative_eq;
use camera::ArcBallCamera;
use geometry::{algorithm::bisect_edge, Plane};
use nalgebra::{Point3, Vector3};
use physical_constants::EARTH_RADIUS_KM;
use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
    time::Instant,
};

// Index into the tree vec. Note, debug builds do not handle the struct indirection well,
// so ironically release has better protections against misuse.

/*
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
 */

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TreeIndex(usize);

fn toff(ti: TreeIndex) -> usize {
    ti.0
}

// Index into the patch vec.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct PatchIndex(pub(crate) usize);

fn poff(pi: PatchIndex) -> usize {
    pi.0
}

#[derive(Debug, Copy, Clone)]
struct PatchKey {
    solid_angle: f64,
    patch_index: PatchIndex,
}

impl PatchKey {
    /*
    fn empty(patch_index: PatchIndex) -> Self {
        Self {
            solid_angle: 0f64,
            patch_index,
        }
    }
     */

    fn new(solid_angle: f64, patch_index: PatchIndex) -> Self {
        Self {
            solid_angle,
            patch_index,
        }
    }
}

impl PartialEq for PatchKey {
    fn eq(&self, other: &Self) -> bool {
        self.patch_index == other.patch_index
    }
}

impl Eq for PatchKey {}

impl Ord for PatchKey {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.solid_angle < other.solid_angle {
            Ordering::Less
        } else if self.solid_angle > other.solid_angle {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl PartialOrd for PatchKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Peer {
    peer: TreeIndex,
    opposite_edge: u8,
}

impl Peer {
    fn new(peer: TreeIndex, opposite_edge: u8) -> Self {
        Self {
            peer,
            opposite_edge,
        }
    }

    fn from_icosahedron(raw: (usize, u8)) -> Self {
        let (peer, opposite_edge) = raw;
        assert!(opposite_edge < 3);
        Self {
            peer: TreeIndex(peer),
            opposite_edge,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Root {
    children: [TreeIndex; 20],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct TreeNode {
    children: Option<[TreeIndex; 4]>,
    peers: [Option<Peer>; 3],
    patch_index: PatchIndex,
    parent: TreeIndex,
    level: usize,
}

impl TreeNode {
    fn is_leaf(&self) -> bool {
        !self.is_node()
    }

    fn is_node(&self) -> bool {
        self.children.is_some()
    }

    fn children(&self) -> &[TreeIndex; 4] {
        self.children.as_ref().unwrap()
    }

    fn peer(&self, edge: u8) -> &Option<Peer> {
        &self.peers[edge as usize]
    }

    fn peer_mut(&mut self, edge: u8) -> &mut Option<Peer> {
        &mut self.peers[edge as usize]
    }

    fn peers(&self) -> &[Option<Peer>; 3] {
        &self.peers
    }

    fn patch_index(&self) -> PatchIndex {
        self.patch_index
    }

    fn offset_of_child(&self, child_index: TreeIndex) -> u8 {
        for (i, child) in self
            .children
            .expect("only call offset_of_child on a node")
            .iter()
            .enumerate()
        {
            if *child == child_index {
                return i as u8;
            }
        }
        unreachable!("offset_of_child called with a non-child index")
    }
}

pub(crate) struct PatchTree {
    max_level: usize,
    patches: Vec<Patch>,
    patch_empty_set: BinaryHeap<Reverse<PatchIndex>>,
    tree: Vec<Option<TreeNode>>,
    tree_empty_set: BinaryHeap<Reverse<TreeIndex>>,
    root: Root,
    root_peers: [[Option<Peer>; 3]; 20],

    subdivide_count: usize,
    rejoin_count: usize,
    visit_count: usize,

    split_queue: BinaryHeap<PatchKey>,
    merge_queue: BinaryHeap<Reverse<PatchKey>>,
    cached_viewable_region: [Plane<f64>; 6],
    cached_eye_position: Point3<f64>,
    cached_eye_direction: Vector3<f64>,
}

impl PatchTree {
    pub(crate) fn new(max_level: usize, falloff_coefficient: f64) -> Self {
        let sphere = Icosahedron::new();
        let mut patches = Vec::new();
        let mut tree = Vec::new();
        let mut root = Root {
            children: [TreeIndex(0); 20],
        };
        let mut root_peers = [[None; 3]; 20];
        for (i, face) in sphere.faces.iter().enumerate() {
            root_peers[i] = [
                Some(Peer::from_icosahedron(face.sibling(0))),
                Some(Peer::from_icosahedron(face.sibling(1))),
                Some(Peer::from_icosahedron(face.sibling(2))),
            ];
            tree.push(Some(TreeNode {
                level: 1,
                parent: TreeIndex(0),
                patch_index: PatchIndex(i),
                peers: root_peers[i],
                children: None,
            }));
            root.children[i] = TreeIndex(i);
            let v0 = Point3::from(sphere.verts[face.i0()] * EARTH_RADIUS_KM);
            let v1 = Point3::from(sphere.verts[face.i1()] * EARTH_RADIUS_KM);
            let v2 = Point3::from(sphere.verts[face.i2()] * EARTH_RADIUS_KM);
            let mut p = Patch::new();
            p.change_target(TreeIndex(i), [v0, v1, v2]);
            patches.push(p);
        }

        Self {
            max_level,
            patches,
            patch_empty_set: BinaryHeap::new(),
            tree,
            tree_empty_set: BinaryHeap::new(),
            root,
            root_peers,
            subdivide_count: 0,
            rejoin_count: 0,
            visit_count: 0,
            split_queue: BinaryHeap::new(),
            merge_queue: BinaryHeap::new(),
            cached_viewable_region: [Plane::from_normal_and_distance(
                Vector3::new(1f64, 0f64, 0f64),
                0f64,
            ); 6],
            cached_eye_position: Point3::new(0f64, 0f64, 0f64),
            cached_eye_direction: Vector3::new(1f64, 0f64, 0f64),
        }
    }

    /*
    pub(crate) fn get_patch_node(&self, index: PatchIndex) -> TreeNode {
        self.tree_node(self.get_patch(index).owner())
    }
     */

    pub(crate) fn get_patch(&self, index: PatchIndex) -> &Patch {
        &self.patches[poff(index)]
    }

    fn get_patch_mut(&mut self, index: PatchIndex) -> &mut Patch {
        &mut self.patches[poff(index)]
    }

    pub(crate) fn level_of_patch(&self, patch_index: PatchIndex) -> usize {
        self.patch_node(patch_index).level
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
        self.tree.push(None);
        tree_index
    }

    fn free_tree_node(&mut self, tree_index: TreeIndex) {
        self.tree[toff(tree_index)] = None;
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
        let eye_position = self.cached_eye_position;
        let eye_direction = self.cached_eye_direction;
        self.get_patch_mut(patch_index)
            .update_for_view(&eye_position, &eye_direction);
        self.tree[toff(tree_index)] = Some(TreeNode {
            parent,
            level,
            patch_index,
            peers: [None; 3],
            children: None,
        });
        tree_index
    }

    fn free_leaf(&mut self, leaf_index: TreeIndex) {
        assert!(
            self.tree_node(leaf_index).is_leaf(),
            "trying to free non-leaf"
        );
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

    fn tree_node(&self, tree_index: TreeIndex) -> &TreeNode {
        self.tree[toff(tree_index)].as_ref().unwrap()
    }

    fn tree_node_mut(&mut self, tree_index: TreeIndex) -> &mut TreeNode {
        self.tree[toff(tree_index)].as_mut().unwrap()
    }

    fn tree_patch(&self, tree_index: TreeIndex) -> &Patch {
        self.get_patch(self.tree_node(tree_index).patch_index)
    }

    fn patch_node(&self, patch_index: PatchIndex) -> &TreeNode {
        self.tree_node(self.get_patch(patch_index).owner())
    }

    fn is_mergable_node(&self, node: &TreeNode) -> bool {
        for child in node.children() {
            if !self.tree_node(*child).is_leaf() {
                return false;
            }
        }
        true
    }

    fn update_splittable_cache(&mut self) {
        // FIXME: can we cache this?
        let mut split_queue = BinaryHeap::new();
        std::mem::swap(&mut self.split_queue, &mut split_queue);
        let mut split_vec = split_queue.into_vec();
        for key in split_vec.iter_mut() {
            key.solid_angle = self.get_patch(key.patch_index).solid_angle();
        }
        split_queue = BinaryHeap::from(split_vec);
        std::mem::swap(&mut self.split_queue, &mut split_queue);
    }

    fn remove_patches_from_split_queue(&mut self, removals: &[PatchIndex]) {
        // FIXME: can we cache this?
        let mut split_queue = BinaryHeap::new();
        std::mem::swap(&mut self.split_queue, &mut split_queue);
        let mut split_vec = split_queue.into_vec();
        // FIXME: Use drain_filter to re-use the allocation; currently only available in nightly.
        split_vec = split_vec
            .drain(..)
            .filter(|&key| !removals.contains(&key.patch_index))
            .collect::<Vec<_>>();
        // O(n) rebuild of the queue.
        split_queue = BinaryHeap::from(split_vec);
        std::mem::swap(&mut self.split_queue, &mut split_queue);
    }

    fn max_splittable(&self) -> f64 {
        self.split_queue
            .peek()
            .unwrap_or_else(|| &PatchKey {
                solid_angle: 0f64,
                patch_index: PatchIndex(0),
            })
            .solid_angle
    }

    fn is_splittable_patch_key(&self, key: &PatchKey) -> bool {
        self.patch_node(key.patch_index).is_leaf()
    }

    fn update_mergeable_cache(&mut self) {
        // FIXME: can we cache this?
        let mut merge_queue = BinaryHeap::new();
        std::mem::swap(&mut self.merge_queue, &mut merge_queue);
        let mut merge_vec = merge_queue.into_vec();
        for Reverse(key) in merge_vec.iter_mut() {
            key.solid_angle = self.get_patch(key.patch_index).solid_angle();
        }
        merge_queue = BinaryHeap::from(merge_vec);
        std::mem::swap(&mut self.merge_queue, &mut merge_queue);
    }

    fn remove_patches_from_merge_queue(&mut self, removals: &[PatchIndex]) {
        // FIXME: can we cache this?
        let mut merge_queue = BinaryHeap::new();
        std::mem::swap(&mut self.merge_queue, &mut merge_queue);
        let mut merge_vec = merge_queue.into_vec();
        // FIXME: Use drain_filter to re-use the allocation; currently only available in nightly.
        merge_vec = merge_vec
            .drain(..)
            .filter(|&Reverse(key)| !removals.contains(&key.patch_index))
            .collect::<Vec<_>>();
        // O(n) rebuild of the queue
        merge_queue = BinaryHeap::from(merge_vec);
        std::mem::swap(&mut self.merge_queue, &mut merge_queue);
    }

    fn min_mergeable(&self) -> f64 {
        self.merge_queue
            .peek()
            .unwrap_or_else(|| {
                &Reverse(PatchKey {
                    solid_angle: 0f64,
                    patch_index: PatchIndex(0),
                })
            })
            .0
            .solid_angle
    }

    fn is_mergeable_patch_key(&self, key: &PatchKey) -> bool {
        let node = self.patch_node(key.patch_index);
        node.is_node() && self.is_mergable_node(node)
    }

    pub(crate) fn optimize_for_view(
        &mut self,
        camera: &ArcBallCamera,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        let reshape_start = Instant::now();

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

        // } otherwise {
        //   Continue processing T=T{f-1}.
        //   Update priorities for all elements of Qs, Qm.
        // }
        for patch in self.patches.iter_mut() {
            if patch.is_alive() {
                patch.update_for_view(&self.cached_eye_position, &self.cached_eye_direction);
            }
        }

        // If f=0 {
        //   Let T = the base triangulation.
        //   Clear Qs, Qm.
        //   Compute priorities for T’s triangles and diamonds, then
        //     insert into Qs and Qm, respectively.
        if self.split_queue.is_empty() {
            for &child in &self.root.children {
                let child_patch_index = self.tree_node(child).patch_index();
                if self
                    .get_patch(child_patch_index)
                    .keep(&self.cached_viewable_region, &self.cached_eye_position)
                {
                    self.split_queue
                        .push(self.patch_key_for_patch(child_patch_index));
                }
            }
        }

        // Update split and merge queue caches with updated solid angles.
        self.update_splittable_cache();
        self.update_mergeable_cache();

        // While T is not the target size/accuracy, or the maximum split priority is greater than the minimum merge priority {
        //   If T is too large or accurate {
        //      Identify lowest-priority (T, TB) in Qm.
        //      Merge (T, TB).
        //      Update queues as follows: {
        //        Remove all merged children from Qs.
        //        Add merge parents T, TB to Qs.
        //        Remove (T, TB) from Qm.
        //        Add all newly-mergeable diamonds to Qm.
        //   } otherwise {
        //     Identify highest-priority T in Qs.
        //     Force-split T.
        //     Update queues as follows: {
        //       Remove T and other split triangles from Qs.
        //       Add any new triangles in T to Qs.
        //       Remove from Qm any diamonds whose children were split.
        //       Add all newly-mergeable diamonds to Qm.
        //     }
        //   }
        // }
        // Set Tf = T.

        let target_patch_count = 200;

        let mut tmp = Vec::new();
        self.capture_patches(&mut tmp);
        let mut patch_count = tmp.len();

        // While T is not the target size/accuracy, or the maximum split priority is greater than the minimum merge priority {
        //println!("{}", self.format_tree_display());
        while self.split_queue.len() > 0
            && (patch_count < target_patch_count - 4
                || self.max_splittable() - self.min_mergeable() > 100.0)
        {
            self.check_tree();
            for key in self.split_queue.iter() {
                assert!(self.is_splittable_patch_key(key));
            }
            for key in self.merge_queue.iter() {
                assert!(self.is_mergeable_patch_key(&key.0));
            }

            // println!(
            //     "{}| {} <- {} vs {}",
            //     self.split_queue.len(),
            //     self.max_splittable() - self.min_mergeable(),
            //     self.max_splittable(),
            //     self.min_mergeable(),
            // );
            //   If T is too large or accurate {
            if patch_count >= target_patch_count {
                if self.merge_queue.len() == 0 {
                    println!("WOULD MERGE");
                    break;
                }

                //      Identify lowest-priority (T, TB) in Qm.
                let bottom_key = self.merge_queue.pop().unwrap().0;
                let smallest_mergeable = bottom_key.patch_index;

                //      Merge (T, TB).
                let node_index = self.get_patch(smallest_mergeable).owner();
                let node = *self.tree_node(self.get_patch(smallest_mergeable).owner());
                let split_removals = node
                    .children()
                    .iter()
                    .map(|ti| self.tree_node(*ti).patch_index)
                    .collect::<Vec<_>>();
                self.rejoin_leaf_patch_into(node.parent, node.level, node_index, node.children());

                //      Update queues as follows: {
                //        Remove all merged children from Qs.
                self.remove_patches_from_split_queue(&split_removals);
                //        Add merge parents T, TB to Qs.
                self.split_queue.push(self.patch_key_for_node(node_index));
                //        Remove (T, TB) from Qm.
                self.remove_patches_from_merge_queue(&[node.patch_index]);
                //        Add all newly-mergeable diamonds to Qm.
                for maybe_node in &self.tree {
                    if let Some(node) = maybe_node {
                        if node.is_node() && self.is_mergable_node(node) {
                            println!("ADD MERGABLE!");
                            self.merge_queue
                                .push(Reverse(self.patch_key_for_patch(node.patch_index)));
                        }
                    }
                }
            } else {
                // Identify highest-priority T in Qs.
                let top_key = self.split_queue.pop().unwrap();
                let biggest_patch = top_key.patch_index;

                // Force-split T.
                // FIXME: use smallvec for these
                let mut split_patches = Vec::new();
                let mut touched_patches = Vec::new();
                self.subdivide_patch(biggest_patch, &mut split_patches, &mut touched_patches);

                // Update queues as follows: {
                //   Remove T and other split triangles from Qs
                self.remove_patches_from_split_queue(&split_patches);
                //   Add any new triangles in T to Qs.
                //   Add all newly-mergeable diamonds to Qm.
                let mut unmergeable = vec![biggest_patch];
                for &key in &touched_patches {
                    let node = self.patch_node(key.patch_index);
                    if node.is_leaf() && node.level < self.max_level {
                        println!("adding split <- {:?}", key);
                        self.split_queue.push(key);
                    } else if node.is_node() {
                        if self.is_mergable_node(&node) {
                            println!("adding merge <- {:?}", key);
                            self.merge_queue.push(Reverse(key));
                        } else {
                            println!("removing merge <- {:?}", key);
                            unmergeable.push(key.patch_index);
                        }
                    }
                }
                for maybe_node in &self.tree {
                    if let Some(node) = maybe_node {
                        if node.is_node() && self.is_mergable_node(node) {
                            println!("ADD MERGABLE!");
                            self.merge_queue
                                .push(Reverse(self.patch_key_for_patch(node.patch_index)));
                        }
                    }
                }
                //   Remove from Qm any diamonds whose children were split.
                self.remove_patches_from_merge_queue(&unmergeable);
            }

            let mut tmp = Vec::new();
            self.capture_patches(&mut tmp);
            patch_count = tmp.len();
            // PANIC!
            //break;
        }

        // Build a view-direction independent tesselation based on the current camera position.
        //self.apply_distance_function();
        self.capture_patches(live_patches);
        let reshape_time = Instant::now() - reshape_start;

        // Select patches based on visibility.
        println!(
            "patches: {} of {} [-{}] | nodes: {} [-{}] | -/+: {}/{}/{} | {:.02}/{:.02} | {:?}",
            live_patches.len(),
            self.patches.len(),
            self.patch_empty_set.len(),
            self.tree.len(),
            self.tree_empty_set.len(),
            self.rejoin_count,
            self.subdivide_count,
            self.visit_count,
            self.max_splittable(),
            self.min_mergeable(),
            reshape_time,
        );
    }

    fn rejoin_leaf_patch_into(
        &mut self,
        parent_index: TreeIndex,
        level: usize,
        tree_index: TreeIndex,
        children: &[TreeIndex; 4],
    ) {
        self.rejoin_count += 1;

        // Clear peer's backref links before we free the leaves.
        // Note: skip inner child links
        for &child in children {
            // Note: skip edges to inner child (always at 1 because 0 vertex faces outwards)
            for j in &[0u8, 2u8] {
                self.update_peer_reverse_pointer(*self.tree_node(child).peer(*j), None);
            }
        }

        // Free the other 3 node/leaf pairs.
        self.free_leaf(children[0]);
        self.free_leaf(children[1]);
        self.free_leaf(children[2]);
        self.free_leaf(children[3]);

        // Replace the current node patch as a leaf patch and free the prior leaf node.
        let node_peers = *self.tree_node(tree_index).peers();
        // println!(
        //     "rejoining {:?} into {:?} <- {:?}",
        //     children, tree_index, node_peers
        // );
        *self.tree_node_mut(tree_index) = TreeNode {
            patch_index: self.tree_node(tree_index).patch_index(),
            parent: parent_index,
            level,
            peers: node_peers,
            children: None,
        };
    }

    fn subdivide_patch(
        &mut self,
        patch_index: PatchIndex,
        split_patches: &mut Vec<PatchIndex>,
        touched_patches: &mut Vec<PatchKey>,
    ) {
        self.subdivide_count += 1;

        println!(
            "subdivide: {:?}; neighborhood: {:?}",
            self.get_patch(patch_index).owner(),
            self.patch_node(patch_index)
                .peers
                .iter()
                .map(|mp| mp.map(|p| Some(p.peer.0)))
                .collect::<Vec<_>>()
        );
        //println!("{}", self.format_tree_display());
        //let node = self.patch_node(patch_index);
        //let node_peers = node.peers;
        //let node_parent = node.parent;
        for own_edge_offset in 0u8..3u8 {
            // println!( "At edge: {} => {:?}",
            //     i,
            //     self.patch_node(patch_index).peer(i)
            // );
            if let Some(peer) = self.patch_node(patch_index).peer(own_edge_offset) {
                // TODO:
            } else {
                let parent = self.tree_node(self.patch_node(patch_index).parent);
                let offset_in_parent = parent.offset_of_child(self.get_patch(patch_index).owner());
                let parent_edge_offset =
                    self.find_parent_edge_for_child(own_edge_offset, offset_in_parent);
                let parent_peer = parent
                    .peer(parent_edge_offset)
                    .expect("parent peer must be present")
                    .peer;

                // If we have no peer, we're next to a larger triangle and need to subdivide it
                // before moving forward.

                // Note: the edge on self does not correspond to the parent edge.
                let peer_node = self.tree_node(parent_peer);
                println!("  parent: {:?}", self.patch_node(patch_index).parent);
                println!("  parent_peer: {:?}", parent_peer);
                assert!(peer_node.is_leaf());
                self.check_tree();
                self.subdivide_patch(peer_node.patch_index, split_patches, touched_patches);
                self.check_tree();
            }
        }

        let patch = self.get_patch(patch_index);
        let node = self.patch_node(patch_index);

        assert!(node.is_leaf());
        let current_level = node.level;
        let next_level = current_level + 1;
        assert!(next_level <= self.max_level);
        let owner = patch.owner();
        let [v0, v1, v2] = patch.points().to_owned();
        let parent = node.parent;
        let leaf_peers = *node.peers();

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

        // Note: we can't fill out inner peer info until after we create children anyway, so just
        // do the entire thing as a post-pass.
        let child_peers = self.make_children_peers(&children, &leaf_peers);
        for (&child_index, &peers) in children.iter().zip(&child_peers) {
            self.tree_node_mut(child_index).peers = peers;
        }

        // Tell our neighbors that we have moved in.
        // Note: skip inner child
        for i in 0..3 {
            // Note: skip edges to inner child (always at 1 because 0 vertex faces outwards)
            for j in &[0, 2] {
                let peer = Some(Peer::new(children[i], *j as u8));
                //println!("  update: {:?} <- {:?}", child_peers[i][*j], peer);
                self.update_peer_reverse_pointer(child_peers[i][*j], peer);
            }
        }

        // println!("subdivided {:?} into {:?}", owner, children);

        println!("    NOTING: {:?}", self.patch_key_for_patch(patch_index));
        split_patches.push(patch_index);
        for i in &children {
            println!("    NOTING: {:?}", self.patch_key_for_node(*i));
            touched_patches.push(self.patch_key_for_node(*i));
        }

        // Transform our leaf/patch into a node and clobber the old patch.
        *self.tree_node_mut(owner) = TreeNode {
            children: Some(children),
            parent,
            patch_index,
            level: current_level,
            peers: leaf_peers,
        };
    }

    fn patch_key_for_node(&self, tree_index: TreeIndex) -> PatchKey {
        self.patch_key_for_patch(self.tree_node(tree_index).patch_index())
    }

    fn patch_key_for_patch(&self, patch_index: PatchIndex) -> PatchKey {
        PatchKey::new(self.get_patch(patch_index).solid_angle(), patch_index)
    }

    fn update_peer_reverse_pointer(
        &mut self,
        maybe_peer: Option<Peer>,
        new_reverse_peer: Option<Peer>,
    ) {
        if let Some(peer) = maybe_peer {
            *self.tree_node_mut(peer.peer).peer_mut(peer.opposite_edge) = new_reverse_peer;
        }
    }

    // FIXME: rename
    fn check_edge_consistency(
        &self,
        tree_index: TreeIndex,
        own_edge_offset: u8,
        edge: &Option<Peer>,
    ) {
        fn assert_points_relative_eq(p0: Point3<f64>, p1: Point3<f64>) {
            assert_relative_eq!(p0.coords[0], p1.coords[0], max_relative = 0.000_001);
            assert_relative_eq!(p0.coords[1], p1.coords[1], max_relative = 0.000_001);
            assert_relative_eq!(p0.coords[2], p1.coords[2], max_relative = 0.000_001);
        }

        let own_patch = self.tree_patch(tree_index);
        if let Some(peer) = edge {
            let peer_patch = self.tree_patch(peer.peer);
            let (s0, s1) = own_patch.edge(own_edge_offset);
            let (p0, p1) = peer_patch.edge(peer.opposite_edge);
            assert_points_relative_eq(s0, p1);
            assert_points_relative_eq(s1, p0);
        } else {
            // If the edge is None, the matching parent edge must not be None. This would imply
            // that the edge is adjacent to something more than one level of subdivision away
            // from it and thus that we would not be able to find an appropriate index buffer
            // winding to merge the levels.
            let parent = self.tree_node(self.tree_node(tree_index).parent);
            let offset_in_parent = parent.offset_of_child(tree_index);
            let parent_edge_offset =
                self.find_parent_edge_for_child(own_edge_offset, offset_in_parent);
            let parent_peer = parent.peer(parent_edge_offset);
            assert!(parent_peer.is_some(), "adjacent subdivision level overflow");
        }
    }

    fn find_parent_edge_for_child(&self, child_edge_offset: u8, child_offset_in_parent: u8) -> u8 {
        // Note: must only be called when the peer at own_edge_offset is None. This implies
        // that the edge we are visiting is more subdivided than the adjacent peer.
        assert_ne!(
            child_offset_in_parent, 3,
            "inner child should never have null edges"
        );

        // The first edge is inner, so must always be at least as subdivided as the outer child.
        assert_ne!(child_edge_offset, 1, "found no-peer-edge on inside edge");

        // On the 0'th edge, the child offset in the parent lines up with the parent edges.
        // On the 2'nd edge, the child offset in the parent is on the opposite side, so offset 2.
        // Ergo: (child_edge_offset + child_offset_in_parent) % 3
        (child_edge_offset + child_offset_in_parent) % 3
    }

    fn child_inner_peer_of(&self, peer: TreeIndex, opposite_edge: u8) -> Option<Peer> {
        Some(Peer {
            peer,
            opposite_edge,
        })
    }

    fn make_children_peers(
        &self,
        children: &[TreeIndex; 4],
        peers: &[Option<Peer>; 3],
    ) -> [[Option<Peer>; 3]; 4] {
        [
            [
                self.child_peer_of(peers[0], 1, 2),
                self.child_inner_peer_of(children[3], 0),
                self.child_peer_of(peers[2], 0, 0),
            ],
            [
                self.child_peer_of(peers[1], 1, 2),
                self.child_inner_peer_of(children[3], 1),
                self.child_peer_of(peers[0], 0, 0),
            ],
            [
                self.child_peer_of(peers[2], 1, 2),
                self.child_inner_peer_of(children[3], 2),
                self.child_peer_of(peers[1], 0, 0),
            ],
            [
                self.child_inner_peer_of(children[0], 1),
                self.child_inner_peer_of(children[1], 1),
                self.child_inner_peer_of(children[2], 1),
            ],
        ]
    }

    // Which child is adjacent depends on what edge of the peer is adjacent to us. Because we are
    // selecting children anti-clockwise and labeling edges anti-clockwise, we can start with a
    // base peer assuming edge 0 is adjacent and bump by the edge offset to get the child peer
    // offset. Yes, this *is* weird, at least until you spend several hours caffinated beyond safe
    // limits. There are likely more useful patterns lurking.
    //
    // Because the zeroth index of every triangle in a subdivision faces outwards, opposite edge
    // is going to be the same no matter what way the triangle is facing, thus we only need one
    // `child_edge` indicator, independent of facing.
    fn child_peer_of(
        &self,
        maybe_peer: Option<Peer>,
        base_child_index: usize,
        child_edge: u8,
    ) -> Option<Peer> {
        if let Some(peer) = maybe_peer {
            let peer_node = self.tree_node(peer.peer);
            if peer_node.is_leaf() {
                return None;
            }
            let adjacent_child = (base_child_index + peer.opposite_edge as usize) % 3;
            return Some(Peer {
                peer: peer_node.children()[adjacent_child],
                opposite_edge: child_edge,
            });
        }
        None
    }

    fn check_tree(&self) {
        // We have already applied visibility at this level, so we just need to recurse.
        let children = self.root.children; // Clone to avoid dual-borrow.
        for i in &children {
            let peers = self.root_peers[toff(*i)];
            self.check_tree_inner(1, *i, &peers);
        }
    }

    fn check_tree_inner(&self, level: usize, tree_index: TreeIndex, peers: &[Option<Peer>; 3]) {
        let node = self.tree_node(tree_index);

        {
            for (i, stored_peer) in peers.iter().enumerate() {
                assert_eq!(*node.peer(i as u8), *stored_peer);
                self.check_edge_consistency(tree_index, 0, &peers[0]);
            }
        }

        if node.is_node() {
            let children_peers = self.make_children_peers(node.children(), peers);
            for (child, child_peers) in node.children().iter().zip(&children_peers) {
                self.check_tree_inner(level + 1, *child, child_peers);
            }
        } else {
            assert!(level <= self.max_level);
        }
    }

    fn capture_patches(&self, live_patches: &mut Vec<PatchIndex>) {
        // We have already applied visibility at this level, so we just need to recurse.
        let children = self.root.children; // Clone to avoid dual-borrow.
        for i in &children {
            self.capture_live_patches_inner(1, *i, live_patches);
        }
    }

    fn capture_live_patches_inner(
        &self,
        level: usize,
        tree_index: TreeIndex,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        let node = self.tree_node(tree_index);

        if node.is_node() {
            for &child in node.children() {
                self.capture_live_patches_inner(level + 1, child, live_patches);
            }
        } else {
            // Don't split leaves past max level.
            assert!(level <= self.max_level);
            // TODO: Note need to pull out index variant from peer info stored on node.
            live_patches.push(node.patch_index);
        }
    }

    #[allow(unused)]
    fn format_tree_display(&self) -> String {
        let mut out = String::new();
        out += "Root\n";
        for child in &self.root.children {
            out += &self.format_tree_display_inner(1, self.tree_node(*child));
        }
        out
    }

    #[allow(unused)]
    fn format_tree_display_inner(&self, lvl: usize, node: &TreeNode) -> String {
        let mut out = String::new();
        if node.is_node() {
            let pad = "  ".repeat(lvl);
            out += &format!("{}Node: {:?}\n", pad, node.children);
            for child in node.children() {
                out += &self.format_tree_display_inner(lvl + 1, self.tree_node(*child));
            }
        } else {
            let pad = "  ".repeat(lvl);
            out += &format!("{}Leaf @{}, lvl: {}\n", pad, poff(node.patch_index), lvl);
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
