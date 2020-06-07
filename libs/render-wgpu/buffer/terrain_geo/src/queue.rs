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
    patch::Patch,
    patch_tree::{toff, PatchTree, TreeIndex, TreeNode},
};
use float_ord::FloatOrd;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet},
    fmt,
};

pub(crate) trait QueueItem {
    fn new(cached_value: f64, tree_index: TreeIndex) -> Self;
    fn tree_index(&self) -> TreeIndex;
    fn solid_angle(&self) -> f64;
    fn set_solid_angle(&mut self, value: f64);
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct MaxHeap {
    cached_value: FloatOrd<f64>,
    tree_index: TreeIndex,
}

impl QueueItem for MaxHeap {
    fn new(cached_value: f64, tree_index: TreeIndex) -> Self {
        Self {
            cached_value: FloatOrd(cached_value),
            tree_index,
        }
    }

    fn tree_index(&self) -> TreeIndex {
        self.tree_index
    }

    fn solid_angle(&self) -> f64 {
        self.cached_value.0
    }

    fn set_solid_angle(&mut self, value: f64) {
        self.cached_value = FloatOrd(value);
    }
}

impl fmt::Debug for MaxHeap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{:.02}", toff(self.tree_index), self.cached_value.0)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct MinHeap {
    cached_value: Reverse<FloatOrd<f64>>,
    tree_index: TreeIndex,
}

impl QueueItem for MinHeap {
    fn new(cached_value: f64, tree_index: TreeIndex) -> Self {
        Self {
            cached_value: Reverse(FloatOrd(cached_value)),
            tree_index,
        }
    }

    fn tree_index(&self) -> TreeIndex {
        self.tree_index
    }

    fn solid_angle(&self) -> f64 {
        (self.cached_value.0).0
    }

    fn set_solid_angle(&mut self, value: f64) {
        self.cached_value = Reverse(FloatOrd(value));
    }
}

impl fmt::Debug for MinHeap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{:.02}",
            toff(self.tree_index),
            (self.cached_value.0).0
        )
    }
}

// The core of the queue is a binary heap. Insertions go right into the heap.
// There is an additional side HashSet of deletions so that we can skip removed
// items in the output stream without having to constantly re-build the heap
// for removals. A second side-hash records contents so that we can dedup
// insertions.
//
// Note that this is intentionally *very* tightly coupled with
// the Tree for performance.
pub(crate) struct Queue<T> {
    heap: BinaryHeap<T>,
    contents: HashSet<TreeIndex>,
    removals: HashSet<TreeIndex>,
}

impl<T: QueueItem + Ord + fmt::Debug> Queue<T> {
    pub(crate) fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            contents: HashSet::new(),
            removals: HashSet::new(),
        }
    }

    pub(crate) fn assert_splittable(&self, tree: &PatchTree) {
        assert_eq!(self.removals.len() + self.contents.len(), self.heap.len());
        for key in self.heap.iter() {
            assert!(
                (self.removals.contains(&key.tree_index())
                    && !self.contents.contains(&key.tree_index()))
                    || (self.contents.contains(&key.tree_index())
                        && !self.removals.contains(&key.tree_index()))
            );
        }
        for &ti in &self.contents {
            assert!(tree.is_splittable_node(ti));
        }
    }

    pub(crate) fn assert_mergeable(&self, tree: &PatchTree) {
        assert_eq!(self.removals.len() + self.contents.len(), self.heap.len());
        for key in self.heap.iter() {
            assert!(
                (self.removals.contains(&key.tree_index())
                    && !self.contents.contains(&key.tree_index()))
                    || (self.contents.contains(&key.tree_index())
                        && !self.removals.contains(&key.tree_index()))
            );
        }
        for &ti in &self.contents {
            if !tree.is_mergeable_node(ti) {
                tree.print_tree();
            }
            assert!(tree.is_mergeable_node(ti));
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.heap.len() - self.removals.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn contains(&self, ti: TreeIndex) -> bool {
        self.contents.contains(&ti)
    }

    pub(crate) fn insert(&mut self, ti: TreeIndex, solid_angle: f64) {
        if self.contents.contains(&ti) {
            // Direct, simple duplicate.
            return;
        }
        if self.removals.contains(&ti) {
            // Unremove it and add it back to contents. It's still in the heap.
            self.removals.remove(&ti);
            self.contents.insert(ti);
            return;
        }
        self.contents.insert(ti);
        self.heap.push(T::new(solid_angle, ti));
    }

    pub(crate) fn remove(&mut self, ti: TreeIndex) {
        if !self.contents.contains(&ti) {
            // Note: either never added or already in removals
            return;
        }
        self.contents.remove(&ti);
        self.removals.insert(ti);
    }

    pub(crate) fn peek_value(&mut self) -> f64 {
        loop {
            let value = self.heap.peek().expect("empty heap in peek");
            if self.removals.contains(&value.tree_index()) {
                assert!(!self.contents.contains(&value.tree_index()));
                self.removals.remove(&value.tree_index());
                self.heap.pop().unwrap();
                continue;
            }
            return value.solid_angle();
        }
    }

    pub(crate) fn pop(&mut self) -> TreeIndex {
        loop {
            let value = self.heap.pop().expect("empty heap in pop");
            if self.removals.contains(&value.tree_index()) {
                assert!(!self.contents.contains(&value.tree_index()));
                self.removals.remove(&value.tree_index());
                continue;
            }
            self.contents.remove(&value.tree_index());
            return value.tree_index();
        }
    }

    pub(crate) fn update_cache(&mut self, tree: &[Option<TreeNode>], patches: &[Patch]) {
        let mut sandbag = BinaryHeap::new();
        std::mem::swap(&mut self.heap, &mut sandbag);
        let mut heap_vec = sandbag.into_vec();
        for key in heap_vec.iter_mut() {
            if !self.removals.contains(&key.tree_index()) {
                let solid_angle = PatchTree::solid_angle_shared(key.tree_index(), tree, patches);
                key.set_solid_angle(solid_angle);
            }
        }
        // O(n) rebuild of the queue
        sandbag = BinaryHeap::from(heap_vec);
        std::mem::swap(&mut self.heap, &mut sandbag);
    }
}

impl<T: QueueItem + Ord + fmt::Debug> fmt::Debug for Queue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "heap: {:?}\nrems: {:?}\nctnt: {:?}",
            self.heap, self.removals, self.contents
        )
    }
}
