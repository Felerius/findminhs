use crate::data_structures::segtree::{SegTree, SegTreeOp};
use crate::instance::NodeIdx;
use crate::small_indices::SmallIdx;
use log::trace;
use std::cmp::Ordering;
use std::hint::unreachable_unchecked;

#[derive(Debug)]
struct ActivitySegTreeOp;

#[derive(Debug, Copy, Clone, Default)]
struct SegTreeItem {
    /// Activity of the associated node.
    activity: (f64, f64),

    /// Which node this item belongs to.
    ///
    /// This is set to `NodeIdx::INVALID` if the node has been deleted. This
    /// way a deleted node can still receive activity boosts without being
    /// considered for the node with the most activity.
    node_idx: NodeIdx,
}

#[cfg(feature = "activity-positive-only")]
fn combine_activity(pos: f64, _neg: f64) -> f64 {
    pos
}

#[cfg(feature = "activity-negative-only")]
fn combine_activity(_pos: f64, neg: f64) -> f64 {
    neg
}

#[cfg(feature = "activity-sum")]
fn combine_activity(pos: f64, neg: f64) -> f64 {
    pos + neg
}

#[cfg(feature = "activity-max")]
fn combine_activity(pos: f64, neg: f64) -> f64 {
    pos.max(neg)
}

#[cfg(feature = "activity-disable")]
fn combine_activity(_pos: f64, _neg: f64) -> f64 {
    0.0
}

#[cfg(not(any(
    feature = "activity-positive-only",
    feature = "activity-negative-only",
    feature = "activity-sum",
    feature = "activity-max",
    feature = "activity-disable",
)))]
compile_error!("No activity combinator function selected");

impl SegTreeOp for ActivitySegTreeOp {
    type Item = SegTreeItem;

    fn combine(left: &Self::Item, right: &Self::Item) -> Self::Item {
        if !left.node_idx.valid() {
            return *right;
        }
        if !right.node_idx.valid() {
            return *left;
        }

        let left_combined = combine_activity(left.activity.0, left.activity.1);
        let right_combined = combine_activity(right.activity.0, right.activity.1);

        // We only ever add and multiply with constants, so we should never
        // have any NaN's. Check in debug mode, optimize release mode under
        // the above assumption.
        match left_combined.partial_cmp(&right_combined) {
            None => {
                if cfg!(debug) {
                    panic!("Activity value was set to NaN")
                } else {
                    unsafe { unreachable_unchecked() }
                }
            }
            Some(Ordering::Greater) | Some(Ordering::Equal) => *left,
            Some(Ordering::Less) => *right,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Activities {
    bump_factor: f64,
    activities: SegTree<ActivitySegTreeOp>,
}

impl Activities {
    /// Factor by which activities are decayed.
    const DECAY_FACTOR: f64 = 0.99;

    /// Threshold for resetting the bump factor
    const RECALC_THRESHOLD: f64 = 1e100;

    pub fn new(num_nodes: usize) -> Self {
        let activities = (0..num_nodes)
            .map(|idx| SegTreeItem {
                activity: (0.0, 0.0),
                node_idx: NodeIdx::from(idx),
            })
            .collect();
        Self {
            bump_factor: 1.0,
            activities,
        }
    }

    pub fn decay(&mut self) {
        trace!("Decaying all");
        self.bump_factor /= Self::DECAY_FACTOR;
        if self.bump_factor >= Self::RECALC_THRESHOLD {
            trace!("Resetting bump amount");
            let bump_factor = self.bump_factor;
            self.activities.change_all(|item| {
                item.activity.0 /= bump_factor;
                item.activity.1 /= bump_factor;
            });
            self.bump_factor = 1.0;
        }
    }

    pub fn bump(&mut self, node_idx: NodeIdx, amount: (f64, f64)) {
        trace!("Bumping {} by {:?}", node_idx, amount);
        let bump_factor = self.bump_factor;
        self.activities.change(node_idx.idx(), |item| {
            item.activity.0 += amount.0 * bump_factor;
            item.activity.1 += amount.1 * bump_factor;
        });
    }

    pub fn delete(&mut self, node_idx: NodeIdx) {
        self.activities.change(node_idx.idx(), |item| {
            debug_assert!(item.node_idx.valid(), "Node {} was deleted twice", node_idx);
            item.node_idx = NodeIdx::INVALID;
        });
    }

    pub fn restore(&mut self, node_idx: NodeIdx) {
        self.activities.change(node_idx.idx(), |item| {
            debug_assert!(
                !item.node_idx.valid(),
                "Node {} restored without being deleted",
                node_idx
            );
            item.node_idx = node_idx;
        });
    }

    pub fn highest(&self) -> NodeIdx {
        self.activities.root().node_idx
    }
}
