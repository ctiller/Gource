//! Region quadtree over small copyable item keys (port of `core/quadtree.{h,cpp}`).
//!
//! The C++ version stores `QuadItem*` pointers; here items are identified by a
//! key `T` (typically an arena index such as a directory or user id) together
//! with the item's bounds at insertion time. Gource rebuilds its trees every
//! frame, so there is no removal API.

use crate::bounds::Bounds2D;
use glam::Vec2;
use std::collections::HashSet;
use std::hash::Hash;

/// An item stored in the tree with its bounding box.
#[derive(Debug, Clone, Copy)]
struct StoredItem<T> {
    item: T,
    bounds: Bounds2D,
}

/// A quadtree node. Leaves hold items; internal nodes have exactly four children.
#[derive(Debug, Clone)]
struct QuadNode<T> {
    bounds: Bounds2D,
    depth: usize,
    items: Vec<StoredItem<T>>,
    /// Either empty (leaf) or four children indices into `QuadTree::nodes`.
    children: Vec<usize>,
}

/// Port of `QuadTree`.
///
/// Semantics that match the C++ implementation:
/// * `new(bounds, max_node_depth, max_node_items)` creates the root node.
/// * `insert` adds the item to every leaf its bounds overlap, subdividing a
///   leaf into four equal quadrants when it holds more than `max_node_items`
///   items and is shallower than `max_node_depth` (see `QuadNode::addItem`,
///   `allowMoreItems`, `addToChild` in quadtree.cpp for the exact rules,
///   including how existing items are pushed down on subdivision).
/// * The visit functions call the callback **at most once per item per call**
///   (the C++ functors dedupe items that live in several nodes using a set when
///   `node_count != 1`). Visit order follows the C++ traversal order (children
///   in index order, items in insertion order).
#[derive(Debug, Clone)]
pub struct QuadTree<T> {
    nodes: Vec<QuadNode<T>>,
    max_node_depth: usize,
    max_node_items: usize,
    /// Number of (item, leaf) insertions, `item_count` in C++.
    item_count: usize,
    /// Number of distinct items inserted, `unique_item_count` in C++.
    unique_item_count: usize,
    /// Deepest node created, `max_node_depth` stat in C++ debug output.
    deepest_node: usize,
}

impl<T: Copy + Eq + Hash> QuadTree<T> {
    /// `QuadTree(Bounds2D bounds, int max_node_depth, int max_node_items)`.
    pub fn new(bounds: Bounds2D, max_node_depth: usize, max_node_items: usize) -> Self {
        let root = QuadNode {
            bounds,
            depth: 1,
            items: Vec::new(),
            children: Vec::new(),
        };
        Self {
            nodes: vec![root],
            max_node_depth,
            max_node_items,
            item_count: 0,
            unique_item_count: 0,
            deepest_node: 1,
        }
    }

    /// `addItem`: insert an item with its bounds.
    pub fn insert(&mut self, item: T, bounds: Bounds2D) {
        let stored = StoredItem { item, bounds };
        self.add_item_to_node(0, stored);
        self.unique_item_count += 1;
    }

    fn allow_more_items(&self, node_idx: usize) -> bool {
        let node = &self.nodes[node_idx];
        node.children.is_empty()
            && (node.depth >= self.max_node_depth || node.items.len() < self.max_node_items)
    }

    fn add_item_to_node(&mut self, node_idx: usize, item: StoredItem<T>) {
        if self.allow_more_items(node_idx) {
            self.item_count += 1;
            self.nodes[node_idx].items.push(item);
            return;
        }

        if !self.nodes[node_idx].children.is_empty() {
            self.add_to_child(node_idx, item);
            return;
        }

        // Subdivide leaf into 4 children
        let bounds = self.nodes[node_idx].bounds;
        let depth = self.nodes[node_idx].depth;
        let average = bounds.centre();
        let middle = average - bounds.min;
        let relmax = bounds.max - bounds.min;

        // 4 quadrants matching C++ QuadNode::addItem:
        // top-left
        let tl_bounds =
            Bounds2D::from_points(bounds.min + Vec2::new(0.0, 0.0), bounds.min + middle);
        // top-right
        let tr_bounds = Bounds2D::from_points(
            bounds.min + Vec2::new(middle.x, 0.0),
            bounds.min + Vec2::new(relmax.x, middle.y),
        );
        // bottom-left
        let bl_bounds = Bounds2D::from_points(
            bounds.min + Vec2::new(0.0, middle.y),
            bounds.min + Vec2::new(middle.x, relmax.y),
        );
        // bottom-right
        let br_bounds = Bounds2D::from_points(bounds.min + middle, bounds.max);

        let child_depth = depth + 1;
        if child_depth > self.deepest_node {
            self.deepest_node = child_depth;
        }

        let first_child_idx = self.nodes.len();
        let child_boxes = [tl_bounds, tr_bounds, bl_bounds, br_bounds];
        for b in child_boxes {
            self.nodes.push(QuadNode {
                bounds: b,
                depth: child_depth,
                items: Vec::new(),
                children: Vec::new(),
            });
        }

        let children_indices = vec![
            first_child_idx,
            first_child_idx + 1,
            first_child_idx + 2,
            first_child_idx + 3,
        ];
        self.nodes[node_idx].children = children_indices;

        // Drain items from current node and push down to children
        let existing_items = std::mem::take(&mut self.nodes[node_idx].items);
        self.item_count -= existing_items.len();

        for old_item in existing_items {
            self.add_to_child(node_idx, old_item);
        }

        // Add the new item to children
        self.add_to_child(node_idx, item);
    }

    fn add_to_child(&mut self, node_idx: usize, item: StoredItem<T>) {
        if self.nodes[node_idx].children.is_empty() {
            return;
        }
        let children = self.nodes[node_idx].children.clone();
        for &child_idx in &children {
            if self.nodes[child_idx].bounds.overlaps(&item.bounds) {
                self.add_item_to_node(child_idx, item);
            }
        }
    }

    fn get_child_index(&self, node_idx: usize, pos: Vec2) -> Option<usize> {
        let node = &self.nodes[node_idx];
        node.children
            .iter()
            .find(|&&child_idx| self.nodes[child_idx].bounds.contains(pos))
            .copied()
    }

    /// `visitItemsAt`: visit the items stored in the leaf (or leaves) containing
    /// `pos`. Note that, like the C++ code, this does not test the item's own
    /// bounds against `pos`; callers do that.
    pub fn visit_items_at(&self, pos: Vec2, mut visit: impl FnMut(T)) {
        let mut seen = HashSet::new();
        self.visit_items_at_node(0, pos, &mut seen, &mut visit);
    }

    fn visit_items_at_node(
        &self,
        node_idx: usize,
        pos: Vec2,
        seen: &mut HashSet<T>,
        visit: &mut impl FnMut(T),
    ) {
        let node = &self.nodes[node_idx];
        if !node.items.is_empty() {
            for stored in &node.items {
                if seen.insert(stored.item) {
                    visit(stored.item);
                }
            }
        } else if let Some(child_idx) = self.get_child_index(node_idx, pos) {
            self.visit_items_at_node(child_idx, pos, seen, visit);
        }
    }

    /// `visitItemsInBounds`: visit items stored in leaves overlapping `bounds`.
    pub fn visit_items_in_bounds(&self, bounds: &Bounds2D, mut visit: impl FnMut(T)) {
        let mut seen = HashSet::new();
        self.visit_items_in_bounds_node(0, bounds, &mut seen, &mut visit);
    }

    fn visit_items_in_bounds_node(
        &self,
        node_idx: usize,
        bounds: &Bounds2D,
        seen: &mut HashSet<T>,
        visit: &mut impl FnMut(T),
    ) {
        let node = &self.nodes[node_idx];
        if !node.items.is_empty() {
            for stored in &node.items {
                if seen.insert(stored.item) {
                    visit(stored.item);
                }
            }
        } else if !node.children.is_empty() {
            let children = &node.children;
            for &child_idx in children {
                let child = &self.nodes[child_idx];
                if !self.is_node_empty(child_idx) && bounds.overlaps(&child.bounds) {
                    self.visit_items_in_bounds_node(child_idx, bounds, seen, visit);
                }
            }
        }
    }

    fn is_node_empty(&self, node_idx: usize) -> bool {
        let node = &self.nodes[node_idx];
        node.items.is_empty() && node.children.is_empty()
    }

    /// Collect the result of [`QuadTree::visit_items_in_bounds`] into a vector.
    pub fn items_in_bounds(&self, bounds: &Bounds2D) -> Vec<T> {
        let mut out = Vec::new();
        self.visit_items_in_bounds(bounds, |item| out.push(item));
        out
    }

    /// Visit the bounds of every node (for the `outline` debug drawing).
    /// The flag is true for leaves.
    pub fn visit_node_bounds(&self, mut visit: impl FnMut(&Bounds2D, bool)) {
        for node in &self.nodes {
            visit(&node.bounds, node.children.is_empty());
        }
    }

    /// Visit the bounds of every leaf that holds at least one item together with
    /// its items (for the `outlineItems` debug drawing).
    pub fn visit_leaf_items(&self, mut visit: impl FnMut(&Bounds2D, &[T])) {
        for node in &self.nodes {
            if node.children.is_empty() && !node.items.is_empty() {
                let item_keys: Vec<T> = node.items.iter().map(|si| si.item).collect();
                visit(&node.bounds, &item_keys);
            }
        }
    }

    pub fn item_count(&self) -> usize {
        self.item_count
    }

    pub fn unique_item_count(&self) -> usize {
        self.unique_item_count
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[allow(clippy::misnamed_getters)]
    pub fn max_node_depth(&self) -> usize {
        self.deepest_node
    }

    pub fn max_node_items(&self) -> usize {
        self.max_node_items
    }

    #[allow(clippy::misnamed_getters)]
    pub fn max_depth_limit(&self) -> usize {
        self.max_node_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_stats() {
        let root_bounds = Bounds2D::from_points(Vec2::new(-100.0, -100.0), Vec2::new(100.0, 100.0));
        let mut tree: QuadTree<u32> = QuadTree::new(root_bounds, 4, 2);
        assert_eq!(tree.node_count(), 1);
        assert_eq!(tree.item_count(), 0);
        assert_eq!(tree.unique_item_count(), 0);
        assert_eq!(tree.max_node_depth(), 1);
        assert_eq!(tree.max_node_items(), 2);
        assert_eq!(tree.max_depth_limit(), 4);

        let mut visited = Vec::new();
        tree.visit_items_at(Vec2::ZERO, |item| visited.push(item));
        assert!(visited.is_empty());

        let mut node_bounds_count = 0;
        tree.visit_node_bounds(|_, is_leaf| {
            assert!(is_leaf);
            node_bounds_count += 1;
        });
        assert_eq!(node_bounds_count, 1);

        let mut leaf_items_count = 0;
        tree.visit_leaf_items(|_, _| {
            leaf_items_count += 1;
        });
        assert_eq!(leaf_items_count, 0);

        // Verify non-empty leaf visitation
        tree.insert(
            42,
            Bounds2D::from_points(Vec2::new(5.0, 5.0), Vec2::new(10.0, 10.0)),
        );
        let mut populated_count = 0;
        tree.visit_leaf_items(|bounds, items| {
            populated_count += 1;
            assert_eq!(items, &[42]);
            assert!(bounds.contains(Vec2::new(5.0, 5.0)));
        });
        assert_eq!(populated_count, 1);
    }

    #[test]
    fn insertion_below_capacity() {
        let root_bounds = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let mut tree = QuadTree::new(root_bounds, 3, 2);

        let item1_bounds = Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0));
        tree.insert(1, item1_bounds);
        assert_eq!(tree.unique_item_count(), 1);
        assert_eq!(tree.item_count(), 1);
        assert_eq!(tree.node_count(), 1);
        assert_eq!(tree.max_node_depth(), 1);

        let item2_bounds = Bounds2D::from_points(Vec2::new(30.0, 30.0), Vec2::new(40.0, 40.0));
        tree.insert(2, item2_bounds);
        assert_eq!(tree.unique_item_count(), 2);
        assert_eq!(tree.item_count(), 2);
        assert_eq!(tree.node_count(), 1);

        let items = tree.items_in_bounds(&root_bounds);
        assert_eq!(items, vec![1, 2]);
    }

    #[test]
    fn subdivision_and_redistribution() {
        let root_bounds = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        // max_node_depth = 2, max_node_items = 2
        let mut tree = QuadTree::new(root_bounds, 2, 2);

        // Top-left: [0..50, 0..50]
        let b1 = Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0));
        // Top-right: [50..100, 0..50]
        let b2 = Bounds2D::from_points(Vec2::new(60.0, 10.0), Vec2::new(70.0, 20.0));
        tree.insert(1, b1);
        tree.insert(2, b2);
        assert_eq!(tree.node_count(), 1);

        // Inserting 3rd item triggers subdivision
        // Bottom-left: [0..50, 50..100]
        let b3 = Bounds2D::from_points(Vec2::new(10.0, 60.0), Vec2::new(20.0, 70.0));
        tree.insert(3, b3);

        // Root has subdivided: 1 root + 4 children = 5 nodes
        assert_eq!(tree.node_count(), 5);
        assert_eq!(tree.max_node_depth(), 2);
        assert_eq!(tree.unique_item_count(), 3);
        // Each item went to exactly one child quadrant
        assert_eq!(tree.item_count(), 3);

        // Querying at (15, 15) should find item 1
        let mut at_p1 = Vec::new();
        tree.visit_items_at(Vec2::new(15.0, 15.0), |item| at_p1.push(item));
        assert_eq!(at_p1, vec![1]);

        // Querying at (65, 15) should find item 2
        let mut at_p2 = Vec::new();
        tree.visit_items_at(Vec2::new(65.0, 15.0), |item| at_p2.push(item));
        assert_eq!(at_p2, vec![2]);

        // Querying at (15, 65) should find item 3
        let mut at_p3 = Vec::new();
        tree.visit_items_at(Vec2::new(15.0, 65.0), |item| at_p3.push(item));
        assert_eq!(at_p3, vec![3]);

        // Querying bottom-right quadrant should find nothing
        let mut at_br = Vec::new();
        tree.visit_items_at(Vec2::new(75.0, 75.0), |item| at_br.push(item));
        assert!(at_br.is_empty());
    }

    #[test]
    fn item_spanning_multiple_quadrants_is_deduped() {
        let root_bounds = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let mut tree = QuadTree::new(root_bounds, 3, 1);

        // Item 1 is right in the center, overlapping all 4 quadrants [40..60, 40..60]
        let center_box = Bounds2D::from_points(Vec2::new(40.0, 40.0), Vec2::new(60.0, 60.0));
        tree.insert(1, center_box);
        assert_eq!(tree.node_count(), 1);

        // Item 2 forces subdivision
        let b2 = Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0));
        tree.insert(2, b2);

        // Item 1 overlaps all 4 children!
        assert_eq!(tree.unique_item_count(), 2);
        // Item 1 in 4 leaves + Item 2 in 1 leaf = 5 leaf insertions
        assert_eq!(tree.item_count(), 5);

        // Visiting across root bounds must dedupe item 1!
        let items = tree.items_in_bounds(&root_bounds);
        assert_eq!(items.len(), 2);
        assert!(items.contains(&1));
        assert!(items.contains(&2));
    }

    #[test]
    fn max_depth_limit_stops_subdivision() {
        let root_bounds = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        // max depth 1 means root cannot subdivide!
        let mut tree = QuadTree::new(root_bounds, 1, 1);
        tree.insert(
            1,
            Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0)),
        );
        tree.insert(
            2,
            Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0)),
        );
        tree.insert(
            3,
            Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0)),
        );

        assert_eq!(tree.node_count(), 1);
        assert_eq!(tree.item_count(), 3);
        assert_eq!(tree.unique_item_count(), 3);
        assert_eq!(tree.max_node_depth(), 1);

        let items = tree.items_in_bounds(&root_bounds);
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn visit_leaf_items_and_node_bounds() {
        let root_bounds = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let mut tree = QuadTree::new(root_bounds, 2, 1);
        tree.insert(
            10,
            Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0)),
        );
        tree.insert(
            20,
            Bounds2D::from_points(Vec2::new(70.0, 70.0), Vec2::new(80.0, 80.0)),
        );

        let mut leaves = 0;
        let mut internals = 0;
        tree.visit_node_bounds(|_, is_leaf| {
            if is_leaf {
                leaves += 1;
            } else {
                internals += 1;
            }
        });
        assert_eq!(internals, 1);
        assert_eq!(leaves, 4);

        let mut populated_leaves = 0;
        tree.visit_leaf_items(|_, items| {
            populated_leaves += 1;
            assert!(!items.is_empty());
        });
        assert_eq!(populated_leaves, 2);

        // Querying inside subdivided tree with visit_items_at
        let mut at_item10 = Vec::new();
        tree.visit_items_at(Vec2::new(15.0, 15.0), |item| at_item10.push(item));
        assert_eq!(at_item10, vec![10]);

        let mut at_item20 = Vec::new();
        tree.visit_items_at(Vec2::new(75.0, 75.0), |item| at_item20.push(item));
        assert_eq!(at_item20, vec![20]);

        // Querying outside tree bounds
        let mut outside = Vec::new();
        tree.visit_items_at(Vec2::new(-500.0, -500.0), |item| outside.push(item));
        assert!(outside.is_empty());

        assert_eq!(tree.max_node_items(), 1);
        assert_eq!(tree.max_depth_limit(), 2);

        // Exercise Clone and Debug
        let cloned = tree.clone();
        assert_eq!(cloned.node_count(), tree.node_count());
        let debug_str = format!("{:?}", tree);
        assert!(debug_str.contains("QuadTree"));
    }

    #[test]
    fn item_outside_bounds_does_not_insert_into_children() {
        let root_bounds = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        // max_node_items = 1, max_depth = 2
        let mut tree = QuadTree::new(root_bounds, 2, 1);

        // First item inside tree
        tree.insert(
            1,
            Bounds2D::from_points(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0)),
        );
        assert_eq!(tree.item_count(), 1);
        assert_eq!(tree.node_count(), 1);

        // Second item completely OUTSIDE root bounds: [-50..-10, -50..-10]
        // This triggers subdivision of the root because root has 1 item and max_node_items is 1.
        // During subdivision: item 1 is pushed down into top-left child.
        // But item 2 does not overlap ANY child!
        // In C++, item 2 is not added to any child, so item_count becomes 1 (only item 1 in a child)!
        let outside_box = Bounds2D::from_points(Vec2::new(-50.0, -50.0), Vec2::new(-10.0, -10.0));
        tree.insert(2, outside_box);

        assert_eq!(tree.node_count(), 5);
        assert_eq!(tree.unique_item_count(), 2);
        // item 1 is in 1 child, item 2 is in 0 children!
        assert_eq!(tree.item_count(), 1);

        // Querying for items in bounds of root
        let items = tree.items_in_bounds(&root_bounds);
        assert_eq!(items, vec![1]);

        // Querying outside
        let outside_items = tree.items_in_bounds(&outside_box);
        assert!(outside_items.is_empty());
    }

    #[test]
    fn deep_quadtree_splitting() {
        let root_bounds = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(1024.0, 1024.0));
        let mut tree = QuadTree::new(root_bounds, 6, 2);

        // Insert items clustered in bottom-right corner [1000..1020, 1000..1020]
        for i in 0..10 {
            let offset = i as f32;
            let b = Bounds2D::from_points(
                Vec2::new(1000.0 + offset, 1000.0 + offset),
                Vec2::new(1001.0 + offset, 1001.0 + offset),
            );
            tree.insert(i, b);
        }

        assert_eq!(tree.unique_item_count(), 10);
        // Tree should have reached max depth
        assert_eq!(tree.max_node_depth(), 6);
        assert_eq!(tree.max_depth_limit(), 6);

        // All 10 items should be retrievable
        let items = tree.items_in_bounds(&root_bounds);
        assert_eq!(items.len(), 10);

        // Outline and leaf checks
        let mut leaf_count = 0;
        let mut populated_leaf_count = 0;
        tree.visit_node_bounds(|_, is_leaf| {
            if is_leaf {
                leaf_count += 1;
            }
        });
        tree.visit_leaf_items(|_, items| {
            populated_leaf_count += 1;
            assert!(!items.is_empty());
        });
        assert!(leaf_count > 0);
        assert!(populated_leaf_count > 0);

        // Query an empty quadrant in the subdivided tree: top-left [0..100, 0..100]
        let empty_area = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let empty_items = tree.items_in_bounds(&empty_area);
        assert!(empty_items.is_empty());
    }
}
