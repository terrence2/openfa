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
use crate::{
    icosahedron::Icosahedron,
    patch::Patch,
    patch_winding::PatchWinding,
    queue::{MaxHeap, MinHeap, Queue},
};

use absolute_unit::Kilometers;
use approx::assert_relative_eq;
use camera::Camera;
use geometry::{algorithm::bisect_edge, Plane};
use nalgebra::{Point3, Vector3};
use physical_constants::EARTH_RADIUS_KM;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet},
    time::Instant,
};

// It's possible that the current viewpoint just cannot be refined to our target, given
// our other constraints. Since we'll have another chance to optimize next frame, bail
// if we spend too much time optimizing.
const MAX_STUCK_ITERATIONS: usize = 6;

// Adds a sanity checks to the inner loop.
const PARANOIA_MODE: bool = false;

// Index into the tree vec. Note, debug builds do not handle the struct indirection well,
// so ironically release has better protections against misuse.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TreeIndex(pub(crate) usize);

pub(crate) fn toff(ti: TreeIndex) -> usize {
    ti.0
}

// Index into the patch vec.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct PatchIndex(pub(crate) usize);

pub(crate) fn poff(pi: PatchIndex) -> usize {
    pi.0
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Peer {
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
enum TreeHolder {
    Children([TreeIndex; 4]),
    Patch(PatchIndex),
}

// FIXME: remove copy from this; if we need copy we've got problems.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TreeNode {
    holder: TreeHolder,
    peers: [Option<Peer>; 3],
    parent: Option<TreeIndex>,
    level: usize,
}

impl TreeNode {
    pub(crate) fn is_leaf(&self) -> bool {
        !self.is_node()
    }

    fn is_node(&self) -> bool {
        match self.holder {
            TreeHolder::Children(ref _children) => true,
            TreeHolder::Patch(_) => false,
        }
    }

    pub(crate) fn children(&self) -> &[TreeIndex; 4] {
        match self.holder {
            TreeHolder::Children(ref children) => children,
            TreeHolder::Patch(_) => panic!("called children on a leaf node"),
        }
    }

    pub(crate) fn patch_index(&self) -> PatchIndex {
        match self.holder {
            TreeHolder::Children(_) => panic!("called patch_index on a node"),
            TreeHolder::Patch(patch_index) => patch_index,
        }
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

    fn offset_of_child(&self, child_index: TreeIndex) -> u8 {
        for (i, child) in self.children().iter().enumerate() {
            if *child == child_index {
                return i as u8;
            }
        }
        unreachable!("offset_of_child called with a non-child index")
    }
}

pub(crate) struct PatchTree {
    max_level: usize,
    target_refinement: f64,
    desired_patch_count: usize,

    patches: Vec<Patch>,
    patch_empty_set: BinaryHeap<Reverse<PatchIndex>>,
    tree: Vec<Option<TreeNode>>,
    tree_empty_set: BinaryHeap<Reverse<TreeIndex>>,
    root: Root,
    root_peers: [[Option<Peer>; 3]; 20],

    split_queue: Queue<MaxHeap>,
    merge_queue: Queue<MinHeap>,

    subdivide_count: usize,
    rejoin_count: usize,
    visit_count: usize,

    frame_number: usize,
    cached_viewable_region: [Plane<f64>; 6],
    cached_eye_position: Point3<f64>,
    cached_eye_direction: Vector3<f64>,
    cached_visible_patches: usize,
}

impl PatchTree {
    pub(crate) fn new(
        max_level: usize,
        target_refinement: f64,
        desired_patch_count: usize,
    ) -> Self {
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
                parent: None,
                holder: TreeHolder::Patch(PatchIndex(i)),
                peers: root_peers[i],
            }));
            root.children[i] = TreeIndex(i);
            let v0 = Point3::from(sphere.verts[face.i0()] * EARTH_RADIUS_KM);
            let v1 = Point3::from(sphere.verts[face.i1()] * EARTH_RADIUS_KM);
            let v2 = Point3::from(sphere.verts[face.i2()] * EARTH_RADIUS_KM);
            let p = Patch::new(TreeIndex(i), [v0, v1, v2]);
            patches.push(p);
        }

        Self {
            max_level,
            target_refinement,
            desired_patch_count,
            patches,
            patch_empty_set: BinaryHeap::new(),
            tree,
            tree_empty_set: BinaryHeap::new(),
            root,
            root_peers,
            subdivide_count: 0,
            rejoin_count: 0,
            visit_count: 0,
            frame_number: 0,
            split_queue: Queue::new(),
            merge_queue: Queue::new(),
            cached_viewable_region: [Plane::from_normal_and_distance(
                Vector3::new(1f64, 0f64, 0f64),
                0f64,
            ); 6],
            cached_eye_position: Point3::new(0f64, 0f64, 0f64),
            cached_eye_direction: Vector3::new(1f64, 0f64, 0f64),
            cached_visible_patches: 0,
        }
    }

    pub(crate) fn get_patch(&self, index: PatchIndex) -> &Patch {
        &self.patches[poff(index)]
    }

    fn get_patch_mut(&mut self, index: PatchIndex) -> &mut Patch {
        &mut self.patches[poff(index)]
    }

    fn allocate_patch(&mut self) -> PatchIndex {
        if let Some(Reverse(patch_index)) = self.patch_empty_set.pop() {
            return patch_index;
        }
        let patch_index = PatchIndex(self.patches.len());
        self.patches.push(Patch::empty());
        patch_index
    }

    fn allocate_leaf_patch(&mut self, tree_index: TreeIndex, pts: [Point3<f64>; 3]) -> PatchIndex {
        let patch_index = self.allocate_patch();
        self.patches[poff(patch_index)].retarget(tree_index, pts);
        let viewable_region = self.cached_viewable_region;
        let eye_position = self.cached_eye_position;
        let eye_direction = self.cached_eye_direction;
        self.get_patch_mut(patch_index).update_for_view(
            &viewable_region,
            &eye_position,
            &eye_direction,
        );
        if self.get_patch(patch_index).in_view() {
            self.cached_visible_patches += 1;
        }
        patch_index
    }

    fn free_patch(&mut self, patch_index: PatchIndex) {
        if self.get_patch(patch_index).in_view() {
            self.cached_visible_patches -= 1;
        }
        self.patch_empty_set.push(Reverse(patch_index));
    }

    // At the end of a frame, we may have net freed patches, resulting in holes. Note that there
    // will not be many of these, since we target a fixed patch count each frame. Also, since each
    // patch is only referred to by a single leaf node in the tree -- the patch info is only pulled
    // out to make the tree smaller for faster tree traversal -- updating after swap can be done
    // in O(1) time. By removing the holes, we can leap without looking when updating the solid
    // angle in each frame, doubling our performance in the common case.
    fn compact_patches(&mut self) {
        let mut removals = self
            .patch_empty_set
            .iter()
            .map(|item| item.0)
            .collect::<HashSet<PatchIndex>>();
        while let Some(Reverse(empty_index)) = self.patch_empty_set.pop() {
            let mut last_index = PatchIndex(self.patches.len() - 1);
            while removals.contains(&last_index) {
                let _ = self.patches.pop().unwrap();
                last_index = PatchIndex(self.patches.len() - 1);
            }
            if empty_index.0 >= self.patches.len() {
                continue;
            }
            if empty_index != last_index {
                removals.remove(&empty_index);
                self.patches[poff(empty_index)] = self.patches[last_index.0];
                self.tree[self.patches[empty_index.0].owner().0]
                    .as_mut()
                    .unwrap()
                    .holder = TreeHolder::Patch(empty_index);
            }
            let _ = self.patches.pop().unwrap();
        }
        assert!(self.patch_empty_set.is_empty());
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
        parent_index: TreeIndex,
        level: usize,
        pts: [Point3<f64>; 3],
    ) -> TreeIndex {
        let tree_index = self.allocate_tree_node();
        let patch_index = self.allocate_leaf_patch(tree_index, pts);
        self.tree[toff(tree_index)] = Some(TreeNode {
            holder: TreeHolder::Patch(patch_index),
            parent: Some(parent_index),
            level,
            peers: [None; 3],
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

    fn tree_node(&self, tree_index: TreeIndex) -> &TreeNode {
        self.tree[toff(tree_index)].as_ref().unwrap()
    }

    fn tree_node_mut(&mut self, tree_index: TreeIndex) -> &mut TreeNode {
        self.tree[toff(tree_index)].as_mut().unwrap()
    }

    pub(crate) fn tree_patch(&self, tree_index: TreeIndex) -> &Patch {
        self.get_patch(self.tree_node(tree_index).patch_index())
    }

    // Note: shared with queue, which can't borrow us because we own it.
    pub(crate) fn solid_angle_shared(
        tree_index: TreeIndex,
        tree: &[Option<TreeNode>],
        patches: &[Patch],
    ) -> f64 {
        let node = tree[toff(tree_index)].as_ref().expect("dead node in tree");
        assert!(node.is_leaf() || Self::is_leaf_node_shared(tree_index, tree));
        match node.holder {
            TreeHolder::Patch(patch_index) => patches[poff(patch_index)].solid_angle(),
            TreeHolder::Children(ref children) => {
                let n0 = tree[toff(children[0])].as_ref().expect("dead child node");
                let n1 = tree[toff(children[1])].as_ref().expect("dead child node");
                let n2 = tree[toff(children[2])].as_ref().expect("dead child node");
                let n3 = tree[toff(children[3])].as_ref().expect("dead child node");
                let p0 = patches[poff(n0.patch_index())];
                let p1 = patches[poff(n1.patch_index())];
                let p2 = patches[poff(n2.patch_index())];
                let p3 = patches[poff(n3.patch_index())];
                p0.solid_angle()
                    .max(p1.solid_angle())
                    .max(p2.solid_angle())
                    .max(p3.solid_angle())
            }
        }
    }

    pub(crate) fn solid_angle(&self, tree_index: TreeIndex) -> f64 {
        Self::solid_angle_shared(tree_index, &self.tree, &self.patches)
    }

    fn check_queues(&self) {
        self.split_queue.assert_splittable(self);
        self.merge_queue.assert_mergeable(self);
    }

    fn update_splittable_cache(&mut self) {
        self.split_queue.update_cache(&self.tree, &self.patches);
    }

    fn max_splittable(&mut self) -> f64 {
        if self.split_queue.is_empty() {
            return f64::MIN;
        }
        self.split_queue.peek_value()
    }

    pub(crate) fn is_splittable_node(&self, ti: TreeIndex) -> bool {
        let node = self.tree_node(ti);
        node.is_leaf() && node.level < self.max_level
    }

    fn update_mergeable_cache(&mut self) {
        self.merge_queue.update_cache(&self.tree, &self.patches);
    }

    fn min_mergeable(&mut self) -> f64 {
        if self.merge_queue.is_empty() {
            return f64::MAX;
        }
        self.merge_queue.peek_value()
    }

    // Note: shared with queue via solid_angle
    fn is_leaf_node_shared(ti: TreeIndex, tree: &[Option<TreeNode>]) -> bool {
        let node = tree[toff(ti)].as_ref().expect("dead node");
        node.is_node()
            && node.children().iter().all(|child| {
                tree[toff(*child)]
                    .as_ref()
                    .expect("dead child node")
                    .is_leaf()
            })
    }

    fn is_leaf_node(&self, ti: TreeIndex) -> bool {
        Self::is_leaf_node_shared(ti, &self.tree)
    }

    pub(crate) fn is_mergeable_node(&self, ti: TreeIndex) -> bool {
        let node = self.tree_node(ti);
        if node.is_leaf() {
            return false;
        }

        // Check if merging would cause us to violate constraints.
        // All of our peers must be either leafs or the adjacent children
        // must not be subdivided. If one of the adjacent nodes is subdivided
        // then merging this node would cause us to become adjacent to that
        // subdivided child node, which would be bad.
        //
        // Another way to state that would be that if any of our children's
        // external edges is Some and is more subdivided than the child,
        // then we cannot subdivide.
        for &child_index in node.children() {
            let child = self.tree_node(child_index);
            if !child.is_leaf() {
                return false;
            }

            for &peer_offset in &[0u8, 2u8] {
                if let Some(peer) = child.peer(peer_offset) {
                    if self.tree_node(peer.peer).is_node() {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn count_in_view_patches(&self) -> usize {
        self.patches
            .iter()
            .map(|p| if p.in_view() { 1 } else { 0 })
            .sum()
    }

    pub(crate) fn optimize_for_view(
        &mut self,
        camera: &Camera,
        live_patches: &mut Vec<(PatchIndex, PatchWinding)>,
    ) {
        assert!(live_patches.is_empty());
        let reshape_start = Instant::now();

        self.subdivide_count = 0;
        self.rejoin_count = 0;
        self.visit_count = 0;

        // let camera_target = camera.cartesian_target_position::<Kilometers>().vec64();
        // let eye_position = camera.cartesian_eye_position::<Kilometers>().point64();
        // let eye_direction = (camera_target - eye_position.coords).normalize();
        self.cached_eye_position = camera.position::<Kilometers>().point64();
        self.cached_eye_direction = *camera.forward();

        for (i, f) in camera
            .world_space_frustum::<Kilometers>()
            .iter()
            .enumerate()
        {
            self.cached_viewable_region[i] = *f;
        }
        self.cached_viewable_region[5] = Plane::from_normal_and_distance(
            self.cached_eye_position.coords.normalize(),
            (((EARTH_RADIUS_KM * EARTH_RADIUS_KM) / self.cached_eye_position.coords.magnitude())
                - 100f64)
                .min(0f64),
        );

        // If f=0 {
        //   Let T = the base triangulation.
        //   Clear Qs, Qm.
        //   Compute priorities for T’s triangles and diamonds, then
        //     insert into Qs and Qm, respectively.
        if self.frame_number == 0 {
            for &child in &self.root.children {
                self.split_queue
                    .insert(child, self.tree_patch(child).solid_angle());
            }
        }
        self.frame_number += 1;

        // } otherwise {
        //   Continue processing T=T{f-1}.
        //   Update priorities for all elements of Qs, Qm.
        // }
        for patch in self.patches.iter_mut() {
            patch.update_for_view(
                &self.cached_viewable_region,
                &self.cached_eye_position,
                &self.cached_eye_direction,
            )
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

        let target_patch_count = self.desired_patch_count;
        self.cached_visible_patches = self.count_in_view_patches();
        let mut stuck_iterations = 0;

        // While T is not the target size/accuracy, or the maximum split priority is greater than the minimum merge priority {
        while self.max_splittable() - self.min_mergeable() > self.target_refinement
            || self.cached_visible_patches < target_patch_count - 4
            || self.cached_visible_patches > target_patch_count
        {
            if self.max_splittable() == f64::MIN && self.min_mergeable() == f64::MIN {
                break;
            }
            if self.cached_visible_patches >= target_patch_count - 4
                && self.cached_visible_patches <= target_patch_count
            {
                stuck_iterations += 1;
                if stuck_iterations > MAX_STUCK_ITERATIONS {
                    break;
                }
            }
            if PARANOIA_MODE {
                self.check_tree(None);
                self.check_queues();
            }

            //   If T is too large or accurate {
            if self.cached_visible_patches >= target_patch_count {
                if self.merge_queue.is_empty() {
                    panic!("would merge but nothing to merge");
                }

                //      Identify lowest-priority (T, TB) in Qm.
                let bottom_key = self.merge_queue.pop();

                //      Merge (T, TB).
                self.rejoin_leaf_patch_into(bottom_key);
            } else {
                if self.split_queue.is_empty() {
                    panic!("would split but nothing to split");
                }

                // Identify highest-priority T in Qs.
                let top_leaf = self.split_queue.pop();

                // Force-split T.
                self.subdivide_leaf(top_leaf);
            }
        }
        assert!(self.cached_visible_patches <= target_patch_count);

        // Prepare for next frame.
        self.compact_patches();
        self.check_tree(None);
        self.check_queues();

        // Build a view-direction independent tesselation based on the current camera position.
        self.capture_patches(live_patches);
        assert!(live_patches.len() <= target_patch_count);
        assert_eq!(self.cached_visible_patches, live_patches.len());
        let reshape_time = Instant::now() - reshape_start;

        // Select patches based on visibility.
        let max_split = self.max_splittable();
        let min_merge = self.min_mergeable();
        println!(
            "r:{} qs:{} qm:{} p:{} t:{}/{} | -/+: {}/{}/{} | {:.02}/{:.02} | {:?}",
            live_patches.len(),
            self.split_queue.len(),
            self.merge_queue.len(),
            self.patches.len(),
            self.tree.len() - self.tree_empty_set.len(),
            self.tree.len(),
            self.rejoin_count,
            self.subdivide_count,
            self.visit_count,
            max_split,
            min_merge,
            reshape_time,
        );
    }

    fn rejoin_leaf_patch_into(&mut self, tree_index: TreeIndex) {
        // println!(
        //     "MERGING {:?} w/ sa: {}",
        //     tree_index,
        //     self.solid_angle(tree_index)
        // );
        self.rejoin_count += 1;
        assert!(self.is_leaf_node(tree_index));
        let children = *self.tree_node(tree_index).children();

        // Save the corners for use in our new patch.
        let pts = [
            self.tree_patch(children[0]).points()[0],
            self.tree_patch(children[1]).points()[0],
            self.tree_patch(children[2]).points()[0],
        ];

        // Clear peer's backref links before we free the leaves.
        // Note: skip inner child links
        for &child in children.iter().take(3) {
            // Note: skip edges to inner child (always at 1 because 0 vertex faces outwards)
            for j in &[0u8, 2u8] {
                if let Some(child_peer) = self.tree_node(child).peer(*j) {
                    assert!(
                        self.tree_node(child_peer.peer).is_leaf(),
                        "Split should have removed peer parents from mergeable status when splitting."
                    );
                }
                self.update_peer_reverse_pointer(*self.tree_node(child).peer(*j), None);
            }
        }
        // Free children and remove from Qs.
        for &child in &children {
            self.free_leaf(child);
            //        Remove all merged children from Qs.
            self.split_queue.remove(child);
        }
        //        Remove (T, TB) from Qm.
        self.merge_queue.remove(tree_index);

        // Replace the current node patch as a leaf patch and free the prior leaf node.
        let patch_index = self.allocate_leaf_patch(tree_index, pts);
        self.tree_node_mut(tree_index).holder = TreeHolder::Patch(patch_index);

        //        Add merge parents T, TB to Qs.
        self.split_queue
            .insert(tree_index, self.solid_angle(tree_index));

        //        Add all newly-mergeable diamonds to Qm.
        if let Some(parent_index) = self.tree_node(tree_index).parent {
            if self.is_mergeable_node(parent_index) {
                self.merge_queue
                    .insert(parent_index, self.solid_angle(parent_index));
            }
            for i in 0..3 {
                if let Some(peer) = self.tree_node(parent_index).peers[i] {
                    if self.is_mergeable_node(peer.peer) {
                        self.merge_queue
                            .insert(peer.peer, self.solid_angle(peer.peer));
                    }
                }
            }
        }
    }

    fn subdivide_leaf(&mut self, tree_index: TreeIndex) {
        // println!(
        //     "SPLITTING {:?} w/ sa: {}",
        //     tree_index,
        //     self.tree_patch(tree_index).solid_angle()
        // );
        self.subdivide_count += 1;

        for own_edge_offset in 0u8..3u8 {
            if self.tree_node(tree_index).peer(own_edge_offset).is_none() {
                if let Some(parent_index) = self.tree_node(tree_index).parent {
                    let parent = self.tree_node(parent_index);
                    let offset_in_parent = parent.offset_of_child(tree_index);
                    let parent_edge_offset =
                        self.find_parent_edge_for_child(own_edge_offset, offset_in_parent);
                    let parent_peer = parent
                        .peer(parent_edge_offset)
                        .expect("parent peer is absent")
                        .peer;

                    // If we have no peer, we're next to a larger triangle and need to subdivide it
                    // before moving forward.

                    // Note: the edge on self does not correspond to the parent edge.
                    let parent_peer_node = self.tree_node(parent_peer);
                    assert!(parent_peer_node.is_leaf());
                    self.subdivide_leaf(parent_peer);
                }
            }
        }

        let node = self.tree_node(tree_index);
        let patch = self.get_patch(node.patch_index());

        assert!(node.is_leaf());
        let current_level = node.level;
        let next_level = current_level + 1;
        assert!(next_level <= self.max_level);
        let [v0, v1, v2] = patch.points().to_owned();
        let maybe_parent_index = node.parent;
        let leaf_peers = *node.peers();

        // Get new points.
        let a = Point3::from(bisect_edge(&v0.coords, &v1.coords).normalize() * EARTH_RADIUS_KM);
        let b = Point3::from(bisect_edge(&v1.coords, &v2.coords).normalize() * EARTH_RADIUS_KM);
        let c = Point3::from(bisect_edge(&v2.coords, &v0.coords).normalize() * EARTH_RADIUS_KM);

        // Allocate geometry to new patches.
        let children = [
            self.allocate_leaf(tree_index, next_level, [v0, a, c]),
            self.allocate_leaf(tree_index, next_level, [v1, b, a]),
            self.allocate_leaf(tree_index, next_level, [v2, c, b]),
            self.allocate_leaf(tree_index, next_level, [c, a, b]),
        ];

        // Note: we can't fill out inner peer info until after we create children anyway, so just
        // do the entire thing as a post-pass. Also, collect the max solid angle among our children.
        let mut max_solid_angle = f64::MIN;
        let child_peers = self.make_children_peers(&children, &leaf_peers);
        for (&child_index, &peers) in children.iter().zip(&child_peers) {
            self.tree_node_mut(child_index).peers = peers;
            let sa = self.tree_patch(child_index).solid_angle();
            if sa > max_solid_angle {
                max_solid_angle = sa;
            }
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

        //   Remove T and other split triangles from Qs
        self.split_queue.remove(tree_index);

        //   Add any new triangles in T to Qs.
        for &child in &children {
            if next_level < self.max_level {
                self.split_queue
                    .insert(child, self.tree_patch(child).solid_angle());
            }
        }

        // We can now be merged, since our children are leafs.
        //   Add all newly-mergeable diamonds to Qm.
        self.merge_queue.insert(tree_index, max_solid_angle);

        // Transform our leaf/patch into a node and clobber the old patch.
        self.free_patch(self.tree_node(tree_index).patch_index());
        self.tree_node_mut(tree_index).holder = TreeHolder::Children(children);

        // We can no longer merge the parent, since we're now a node instead of a leaf.
        // We can also no longer merge our parent's peers, since that would create
        // a two level split difference after splitting this node.
        //   Remove from Qm any diamonds whose children were split.
        if let Some(parent_index) = maybe_parent_index {
            self.merge_queue.remove(parent_index);
            for i in 0..3 {
                if let Some(peer) = self.tree_node(parent_index).peers[i] {
                    if !self.is_mergeable_node(peer.peer) {
                        self.merge_queue.remove(peer.peer);
                    }
                }
            }
        }
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

        let own_node = self.tree_node(tree_index);
        if let Some(peer) = edge {
            let other_node = self.tree_node(peer.peer);
            if own_node.is_leaf() && other_node.is_leaf() {
                let own_patch = self.tree_patch(tree_index);
                let peer_patch = self.tree_patch(peer.peer);
                let (s0, s1) = own_patch.edge(own_edge_offset);
                let (p0, p1) = peer_patch.edge(peer.opposite_edge);
                assert_points_relative_eq(s0, p1);
                assert_points_relative_eq(s1, p0);
            } else {
                // TODO: could drill down into children to get points to match
            }
        } else {
            // If the edge is None, the matching parent edge must not be None. This would imply
            // that the edge is adjacent to something more than one level of subdivision away
            // from it and thus that we would not be able to find an appropriate index buffer
            // winding to merge the levels.
            if let Some(parent_index) = self.tree_node(tree_index).parent {
                let parent = self.tree_node(parent_index);
                let offset_in_parent = parent.offset_of_child(tree_index);
                let parent_edge_offset =
                    self.find_parent_edge_for_child(own_edge_offset, offset_in_parent);
                let parent_peer = parent.peer(parent_edge_offset);
                assert!(parent_peer.is_some(), "adjacent subdivision level overflow");
            }
        }
    }

    fn check_tree(&self, split_context: Option<TreeIndex>) {
        //self.print_tree();

        for (i, maybe_node) in self.tree.iter().enumerate() {
            if let Some(node) = maybe_node {
                if i >= 20 {
                    assert!(node.parent.is_some());
                    assert!(self.tree[toff(node.parent.unwrap())].is_some());
                    let parent = self.tree_node(node.parent.unwrap());
                    if parent.is_node() {
                        assert!(parent.children().iter().any(|f| toff(*f) == i));
                    }
                }
            }
        }

        // We have already applied visibility at this level, so we just need to recurse.
        let children = self.root.children; // Clone to avoid dual-borrow.
        for i in &children {
            let peers = self.root_peers[toff(*i)];
            self.check_tree_inner(1, *i, &peers, split_context);
        }
    }

    fn check_tree_inner(
        &self,
        level: usize,
        tree_index: TreeIndex,
        peers: &[Option<Peer>; 3],
        split_context: Option<TreeIndex>,
    ) {
        let node = self.tree_node(tree_index);

        for (i, stored_peer) in peers.iter().enumerate() {
            assert_eq!(*node.peer(i as u8), *stored_peer);
            self.check_edge_consistency(tree_index, i as u8, &peers[i]);
        }

        if node.is_node() {
            if self.is_mergeable_node(tree_index) {
                if !self.merge_queue.contains(tree_index) {
                    println!("{:?} is mergeable, so should be in merge_queue", tree_index);
                    self.print_tree();
                }
                assert!(self.merge_queue.contains(tree_index));
            }
            let children_peers = self.make_children_peers(node.children(), peers);
            for (child, child_peers) in node.children().iter().zip(&children_peers) {
                self.check_tree_inner(level + 1, *child, child_peers, split_context);
            }
        } else {
            assert!(level <= self.max_level);
            if self.is_splittable_node(tree_index) && level < self.max_level {
                if !(self.split_queue.contains(tree_index) || Some(tree_index) == split_context) {
                    println!(
                        "{:?} is splittable, so should be in split_queue; {} < {} w/ ctx {:?}",
                        tree_index, level, self.max_level, split_context
                    );
                    self.print_tree();
                }
                assert!(self.split_queue.contains(tree_index) || Some(tree_index) == split_context);
            }
        }
    }

    fn capture_patches(&self, live_patches: &mut Vec<(PatchIndex, PatchWinding)>) {
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
        live_patches: &mut Vec<(PatchIndex, PatchWinding)>,
    ) {
        let node = self.tree_node(tree_index);

        if node.is_node() {
            for &child in node.children() {
                self.capture_live_patches_inner(level + 1, child, live_patches);
            }
        } else {
            // Don't split leaves past max level.
            assert!(level <= self.max_level);
            if self.get_patch(node.patch_index()).in_view() {
                live_patches.push((node.patch_index(), PatchWinding::from_peers(&node.peers)));
            }
        }
    }

    #[allow(unused)]
    pub(crate) fn print_tree(&self) {
        println!("{}", self.format_tree_display());
    }

    #[allow(unused)]
    fn format_tree_display(&self) -> String {
        let mut out = String::new();
        out += "Root\n";
        for child in &self.root.children {
            out += &self.format_tree_display_inner(1, *child);
        }
        out
    }

    #[allow(unused)]
    fn format_tree_display_inner(&self, lvl: usize, tree_index: TreeIndex) -> String {
        fn fmt_peer(mp: &Option<Peer>) -> String {
            if let Some(p) = mp {
                format!("{}", toff(p.peer))
            } else {
                "x".to_owned()
            }
        }
        let node = self.tree_node(tree_index);
        let mut out = String::new();
        if node.is_node() {
            let pad = "  ".repeat(lvl);
            out += &format!(
                "{}Node @{}, [{},{},{}]\n",
                pad,
                toff(tree_index),
                fmt_peer(&node.peers[0]),
                fmt_peer(&node.peers[1]),
                fmt_peer(&node.peers[2]),
            );
            for child in node.children() {
                out += &self.format_tree_display_inner(lvl + 1, *child);
            }
        } else {
            let pad = "  ".repeat(lvl);
            out += &format!(
                "{}Leaf @{}, peers: [{},{},{}]\n",
                pad,
                poff(node.patch_index()),
                fmt_peer(&node.peers[0]),
                fmt_peer(&node.peers[1]),
                fmt_peer(&node.peers[2]),
            );
        }
        out
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use absolute_unit::{degrees, meters};
    use camera::ArcBallCamera;
    use failure::Fallible;
    use geodesy::{GeoSurface, Graticule, Target};

    #[test]
    fn test_pathological() -> Fallible<()> {
        let mut tree = PatchTree::new(15, 150.0, 300);
        let mut live_patches = Vec::new();
        let mut arcball = ArcBallCamera::new(16.0 / 9.0, meters!(0.1), meters!(10_000));
        arcball.set_eye_relative(Graticule::<Target>::new(
            degrees!(89),
            degrees!(0),
            meters!(4_000_000),
        ))?;

        arcball.set_target(Graticule::<GeoSurface>::new(
            degrees!(0),
            degrees!(0),
            meters!(2),
        ));
        live_patches.clear();
        tree.optimize_for_view(arcball.camera(), &mut live_patches);

        arcball.set_target(Graticule::<GeoSurface>::new(
            degrees!(0),
            degrees!(180),
            meters!(2),
        ));
        live_patches.clear();
        tree.optimize_for_view(arcball.camera(), &mut live_patches);

        arcball.set_target(Graticule::<GeoSurface>::new(
            degrees!(0),
            degrees!(0),
            meters!(2),
        ));
        live_patches.clear();
        tree.optimize_for_view(arcball.camera(), &mut live_patches);
        Ok(())
    }

    #[test]
    fn test_zoom_in() -> Fallible<()> {
        let mut tree = PatchTree::new(15, 150.0, 300);
        let mut live_patches = Vec::new();
        let mut arcball = ArcBallCamera::new(16.0 / 9.0, meters!(0.1), meters!(10_000));
        arcball.set_target(Graticule::<GeoSurface>::new(
            degrees!(0),
            degrees!(0),
            meters!(2),
        ));

        const CNT: i64 = 40;
        for i in 0..CNT {
            arcball.set_eye_relative(Graticule::<Target>::new(
                degrees!(89),
                degrees!(0),
                meters!(4_000_000 - i * (4_000_000 / CNT)),
            ))?;
            live_patches.clear();
            tree.optimize_for_view(arcball.camera(), &mut live_patches);
        }

        Ok(())
    }

    #[test]
    fn test_fly_forward() -> Fallible<()> {
        let mut tree = PatchTree::new(15, 150.0, 300);
        let mut live_patches = Vec::new();
        let mut arcball = ArcBallCamera::new(16.0 / 9.0, meters!(0.1), meters!(10_000));
        arcball.set_target(Graticule::<GeoSurface>::new(
            degrees!(0),
            degrees!(0),
            meters!(1000),
        ));
        arcball.set_eye_relative(Graticule::<Target>::new(
            degrees!(1),
            degrees!(90),
            meters!(1_500_000),
        ))?;

        const CNT: i64 = 40;
        for i in 0..CNT {
            arcball.set_target(Graticule::<GeoSurface>::new(
                degrees!(0),
                degrees!(4 * i),
                meters!(1000),
            ));
            live_patches.clear();
            tree.optimize_for_view(arcball.camera(), &mut live_patches);
        }

        Ok(())
    }
}
