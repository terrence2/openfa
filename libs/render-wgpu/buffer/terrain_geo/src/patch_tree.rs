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
use failure::_core::hint::unreachable_unchecked;
use geometry::{IcoSphere, Plane};
use nalgebra::{Point3, Vector3};
use physical_constants::EARTH_RADIUS_KM;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    time::Instant,
};
use wgpu::BindingType::UniformBuffer;

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct PatchIndex(pub(crate) u32);

impl PatchIndex {
    fn new(i: usize) -> Self {
        Self(i as u32)
    }
}

fn p(patch_index: PatchIndex) -> usize {
    patch_index.0 as usize
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TreeIndex(pub(crate) u32);

impl TreeIndex {
    fn new(i: usize) -> Self {
        Self(i as u32)
    }
}

fn t(tree_index: TreeIndex) -> usize {
    tree_index.0 as usize
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct VertexIndex(pub(crate) u32);

impl VertexIndex {
    fn new(i: usize) -> Self {
        Self(i as u32)
    }
}

fn v(vertex_index: VertexIndex) -> usize {
    vertex_index.0 as usize
}

struct Tree {
    parent: TreeIndex,       // owns the corners
    patch_index: PatchIndex, // duplicates the vertex data...
    corners: [VertexIndex; 3],
    children: Option<[TreeIndex; 4]>,
    level: usize,
}

impl Tree {
    fn new_leaf(
        parent: TreeIndex,
        patch_index: PatchIndex,
        corners: [VertexIndex; 3],
        level: usize,
    ) -> Self {
        Self {
            parent,
            patch_index,
            corners,
            children: None,
            level,
        }
    }

    fn empty() -> Self {
        Self {
            parent: TreeIndex(UNINIT_MARKER),
            patch_index: PatchIndex(UNINIT_MARKER),
            corners: [
                VertexIndex(UNINIT_MARKER),
                VertexIndex(UNINIT_MARKER),
                VertexIndex(UNINIT_MARKER),
            ],
            children: None,
            level: usize::MAX,
        }
    }

    fn children_for_edge(&self, edge_offset: u8) -> Option<(TreeIndex, TreeIndex)> {
        println!("eo: {}", edge_offset);
        assert!(edge_offset < 3);
        if let Some(ref children) = self.children {
            Some(match edge_offset {
                0 => (children[0], children[1]),
                1 => (children[1], children[2]),
                2 => (children[2], children[0]),
                _ => unreachable!(),
            })
        } else {
            None
        }
    }

    fn edge(&self, offset: u8) -> EdgeIndex {
        assert!(offset < 3);
        match offset {
            0 => EdgeIndex::new(self.corners[0], self.corners[1]),
            1 => EdgeIndex::new(self.corners[1], self.corners[2]),
            2 => EdgeIndex::new(self.corners[2], self.corners[0]),
            _ => unreachable!(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct EdgeIndex {
    vertices: [VertexIndex; 2],
}

impl EdgeIndex {
    fn new(vertex0: VertexIndex, vertex1: VertexIndex) -> Self {
        assert_ne!(vertex0.0, vertex1.0);
        if vertex0.0 < vertex1.0 {
            Self {
                vertices: [vertex0, vertex1],
            }
        } else {
            Self {
                vertices: [vertex1, vertex0],
            }
        }
    }
}

const UNINIT_MARKER: u32 = u32::MAX;
const UNINIT_MARKER_8: u8 = u8::MAX;

struct EdgePair {
    sides: [TreeIndex; 2],
    peers: [u8; 2],
}

impl EdgePair {
    fn new(side0: TreeIndex, peer0: u8, side1: TreeIndex, peer1: u8) -> Self {
        assert!(peer0 < 3);
        assert!(peer1 < 3);
        assert_ne!(side0.0, side1.0);
        if side0.0 < side1.0 {
            Self {
                sides: [side0, side1],
                peers: [peer0, peer1],
            }
        } else {
            Self {
                sides: [side1, side0],
                peers: [peer1, peer0],
            }
        }
    }

    fn new_partial(side: TreeIndex, peer: u8) -> Self {
        assert!(peer < 3);
        Self {
            sides: [side, TreeIndex(UNINIT_MARKER)],
            peers: [peer, UNINIT_MARKER_8],
        }
    }

    fn is_incomplete(&self) -> bool {
        self.sides[1].0 == UNINIT_MARKER
    }

    fn complete(&mut self, other_side: TreeIndex, other_peer: u8) {
        assert!(other_peer < 3);
        assert!(self.is_incomplete());
        if self.sides[0].0 < other_side.0 {
            self.sides[1] = other_side;
            self.peers[1] = other_peer;
        } else {
            self.sides[1] = self.sides[0];
            self.peers[1] = self.peers[0];
            self.sides[0] = other_side;
            self.peers[0] = other_peer;
        }
    }

    fn opposite_side(&self, side: TreeIndex) -> (TreeIndex, u8) {
        if side == self.sides[0] {
            return (self.sides[1], self.peers[1]);
        }
        assert_eq!(side, self.sides[1]);
        (self.sides[0], self.peers[0])
    }
}

pub(crate) struct PatchTree {
    vertices: Vec<Vector3<f64>>,
    edges: HashMap<EdgeIndex, EdgePair>,
    tree: Vec<Tree>,
    patches: Vec<Patch>,
    tree_empty_set: BinaryHeap<Reverse<TreeIndex>>,
    patch_empty_set: BinaryHeap<Reverse<PatchIndex>>,

    max_level: usize,
    depth_levels: Vec<f64>,

    cached_viewable_region: [Plane<f64>; 6],
    cached_eye_position: Point3<f64>,
}

impl PatchTree {
    pub(crate) fn new(max_level: usize, falloff_coefficient: f64) -> Self {
        let mut depth_levels = Vec::new();
        for i in 0..=max_level {
            let d = 1f64 * EARTH_RADIUS_KM * 2f64.powf(-(i as f64 * falloff_coefficient));
            depth_levels.push(d * d);
        }

        let sphere = Icosahedron::new();
        let mut vertices = sphere.verts.clone();
        let mut edges = HashMap::new();
        let mut tree = Vec::new();
        let mut patches = Vec::new();
        for (i, face) in sphere.faces.iter().enumerate() {
            let face_edges = [
                EdgeIndex::new(VertexIndex::new(face.i0()), VertexIndex::new(face.i1())),
                EdgeIndex::new(VertexIndex::new(face.i1()), VertexIndex::new(face.i2())),
                EdgeIndex::new(VertexIndex::new(face.i2()), VertexIndex::new(face.i0())),
            ];
            for (j, edge_index) in face_edges.iter().enumerate() {
                let [sibling, peer_edge, ..] = face.siblings[j];
                edges.entry(*edge_index).or_insert(EdgePair::new(
                    TreeIndex::new(i),
                    j as u8,
                    TreeIndex::new(sibling),
                    peer_edge as u8,
                ));
            }
            tree.push(Tree {
                parent: TreeIndex(UNINIT_MARKER),
                patch_index: PatchIndex::new(i),
                corners: [
                    VertexIndex::new(face.i0()),
                    VertexIndex::new(face.i1()),
                    VertexIndex::new(face.i2()),
                ],
                children: None,
                level: 0,
            });
            patches.push(Patch::new());
            patches[i].change_target(
                TreeIndex::new(i),
                [
                    Point3::from(vertices[face.i0()]),
                    Point3::from(vertices[face.i1()]),
                    Point3::from(vertices[face.i2()]),
                ],
            );
        }
        Self {
            vertices,
            edges,
            tree,
            patches,
            tree_empty_set: BinaryHeap::new(),
            patch_empty_set: BinaryHeap::new(),
            max_level,
            depth_levels,
            cached_viewable_region: [Plane::from_normal_and_distance(
                Vector3::new(1f64, 0f64, 0f64),
                0f64,
            ); 6],
            cached_eye_position: Point3::new(0f64, 0f64, 0f64),
        }
    }

    pub(crate) fn optimize_for_view(
        &mut self,
        camera: &ArcBallCamera,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        self.cached_eye_position = camera.cartesian_eye_position::<Kilometers>().point64();
        for (i, f) in camera.world_space_frustum().iter().enumerate() {
            self.cached_viewable_region[i] = *f;
        }
        self.subdivide_to_depth(live_patches);

        // for (i, patch) in self.patches.iter().enumerate() {
        //     live_patches.push(PatchIndex::new(i));
        // }

        println!(
            "v:{} e:{} p:{} t:{}",
            self.vertices.len(),
            self.edges.len(),
            self.patches.len(),
            self.tree.len(),
        );
    }

    fn subdivide_to_depth(&mut self, live_patches: &mut Vec<PatchIndex>) {
        for i in 0..20 {
            self.subdivide_to_depth_inner(TreeIndex::new(i), 1, live_patches);
        }
    }

    fn subdivide_to_depth_inner(
        &mut self,
        tree_index: TreeIndex,
        level: usize,
        live_patches: &mut Vec<PatchIndex>,
    ) {
        if let Some(children) = &self.tree[t(tree_index)].children {
            /*
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
             */
        } else {
            // Don't split leaves past max level.
            assert!(level <= self.max_level);

            if level < self.max_level && self.leaf_is_inside_distance_function(level, tree_index) {
                self.subdivide_leaf(tree_index);
                self.subdivide_to_depth_inner(tree_index, level, live_patches);
                return;
            }

            let patch_index = &self.tree[t(tree_index)].patch_index;
            if self
                .get_patch(*patch_index)
                .keep(&self.cached_viewable_region, &self.cached_eye_position)
            {
                live_patches.push(*patch_index);
            }
        }
    }

    /*
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
    */

    fn subdivide_edge(
        &mut self,
        for_leaf: TreeIndex,
        i0: VertexIndex,
        i1: VertexIndex,
    ) -> VertexIndex {
        // FIXME: what happens we we subdivide too far?

        // Find existing edge: we know it exists because the leaf exists.
        let edge_index = EdgeIndex::new(i0, i1);
        let edge = &self.edges[&edge_index];
        let (opposite_side, opposite_side_peer_edge) = edge.opposite_side(for_leaf);
        let other_node = &self.tree[t(opposite_side)];
        if let Some((child_index_0, child_index_1)) =
            other_node.children_for_edge(opposite_side_peer_edge)
        {
            let child0 = &self.tree[t(child_index_0)];
            let child1 = &self.tree[t(child_index_1)];
            let sub_edge0 = child0.edge(opposite_side_peer_edge);
            let sub_edge1 = child1.edge(opposite_side_peer_edge);
        }
        // if let Some(children) = &other_node.children {
        //     unimplemented!()
        // }
        let v0 = &self.vertices[v(i0)];
        let v1 = &self.vertices[v(i1)];
        let midpoint = IcoSphere::bisect_edge(&v0, &v1).normalize() * EARTH_RADIUS_KM;
        self.allocate_vertex(midpoint)
    }

    fn subdivide_leaf(&mut self, tree_index: TreeIndex) {
        assert!(self.tree[t(tree_index)].children.is_none());
        let level = self.tree[t(tree_index)].level;
        let [i0, i1, i2] = self.tree[t(tree_index)].corners;
        let a = self.subdivide_edge(tree_index, i0, i1);
        let b = self.subdivide_edge(tree_index, i1, i2);
        let c = self.subdivide_edge(tree_index, i2, i0);
        self.node_mut(tree_index).children = Some([
            self.allocate_leaf(tree_index, [i0, a, c], level + 1),
            self.allocate_leaf(tree_index, [i1, b, a], level + 1),
            self.allocate_leaf(tree_index, [i2, c, b], level + 1),
            self.allocate_leaf(tree_index, [a, b, c], level + 1),
        ]);

        /*
        println!("subdivide: {:?}", self.get_patch(patch_index).owner());
        assert!(self
            .tree_node(self.get_patch(patch_index).owner())
            .is_leaf());
        self.subdivide_count += 1;
        let current_level = self.tree_node(self.get_patch(patch_index).owner()).level();
        let next_level = current_level + 1;
        assert!(next_level <= self.max_level);
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
         */
    }

    fn leaf_is_inside_distance_function(&self, level: usize, tree_index: TreeIndex) -> bool {
        assert!(level > 0);
        let patch = self.patch(self.node(tree_index).patch_index);
        let d2 = patch.distance_squared_to(&self.cached_eye_position);
        d2 < self.depth_levels[level]
    }

    fn node(&self, ti: TreeIndex) -> &Tree {
        &self.tree[t(ti)]
    }

    fn node_mut(&mut self, ti: TreeIndex) -> &mut Tree {
        &mut self.tree[t(ti)]
    }

    fn patch(&self, pi: PatchIndex) -> &Patch {
        &self.patches[p(pi)]
    }

    fn patch_mut(&mut self, pi: PatchIndex) -> &mut Patch {
        &mut self.patches[p(pi)]
    }

    fn vertex(&self, vi: VertexIndex) -> Vector3<f64> {
        self.vertices[v(vi)]
    }

    fn allocate_patch_index(&mut self) -> PatchIndex {
        if let Some(Reverse(patch_index)) = self.patch_empty_set.pop() {
            return patch_index;
        }
        let patch_index = PatchIndex::new(self.patches.len());
        self.patches.push(Patch::new());
        patch_index
    }

    fn allocate_tree_index(&mut self) -> TreeIndex {
        if let Some(Reverse(tree_index)) = self.tree_empty_set.pop() {
            return tree_index;
        }
        let tree_index = TreeIndex::new(self.tree.len());
        self.tree.push(Tree::empty());
        tree_index
    }

    fn allocate_leaf(
        &mut self,
        parent: TreeIndex,
        corners: [VertexIndex; 3],
        level: usize,
    ) -> TreeIndex {
        let patch_index = self.allocate_patch_index();
        let tree_index = self.allocate_tree_index();
        let pts = [
            Point3::from(self.vertex(corners[0])),
            Point3::from(self.vertex(corners[1])),
            Point3::from(self.vertex(corners[2])),
        ];
        self.patch_mut(patch_index).change_target(tree_index, pts);
        *self.node_mut(tree_index) = Tree::new_leaf(parent, patch_index, corners, level);
        tree_index
    }

    fn allocate_vertex(&mut self, initial: Vector3<f64>) -> VertexIndex {
        // TODO: recount and remove
        let index = VertexIndex(self.vertices.len() as u32);
        self.vertices.push(initial);
        index
    }

    pub(crate) fn get_patch(&self, index: PatchIndex) -> &Patch {
        &self.patches[p(index)]
    }

    pub(crate) fn level_of_patch(&self, patch_index: PatchIndex) -> usize {
        self.tree[t(self.patches[p(patch_index)].owner())].level
    }
}

/*
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

        let sphere = IcoSphere::new(0);
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
        for i in 0..20 {
            tree.push(TreeNode::Leaf(Leaf {
                level: 1,
                parent: TreeIndex(0),
                patch_index: PatchIndex(i),
            }));
            root.children[i] = TreeIndex(i + 1);
        }
        for (i, face) in sphere.faces.iter().enumerate() {
            let v0 = Point3::from(sphere.verts[face.i0()] * EARTH_RADIUS_KM);
            let v1 = Point3::from(sphere.verts[face.i1()] * EARTH_RADIUS_KM);
            let v2 = Point3::from(sphere.verts[face.i2()] * EARTH_RADIUS_KM);
            let mut p = Patch::new();
            p.change_target(TreeIndex(i + 1), [v0, v1, v2]);
            patches.push(p)
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
        self.apply_distance_function(&eye_position, live_patches);
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
        assert!(next_level <= self.max_level);
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
                for i in &children {
                    self.apply_distance_function_inner(eye_position, level + 1, *i, live_patches);
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
                assert!(level <= self.max_level);

                if level < self.max_level
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

    #[allow(unused)]
    fn format_tree_display(&self) -> String {
        self.format_tree_display_inner(0, self.tree_root())
    }

    #[allow(unused)]
    fn format_tree_display_inner(&self, lvl: usize, node: TreeNode) -> String {
        let mut out = String::new();
        match node {
            TreeNode::Root => {
                out += "Root\n";
                for child in &self.root.children {
                    out += &self.format_tree_display_inner(lvl + 1, self.tree_node(*child));
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
                out += &format!("{}Leaf @{}, lvl: {}\n", pad, poff(leaf.patch_index), lvl);
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
*/
